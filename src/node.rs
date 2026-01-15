use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use jsonrpsee::server::{RpcModule, Server};
use jsonrpsee::{IntoResponse, ResponsePayload};
use litep2p::crypto::PublicKey;
use litep2p::PeerId;
use nvml_wrapper::Nvml;
use serde::Serialize;
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{
        mpsc::{self, Receiver, Sender},
        RwLock,
    },
};
use tracing::{event, span, Level};

use crate::{
    identity::IdentityManager,
    message::{ConnectionReq, ConnectionResp, Message},
    types::{Capabilities, GraphicCard},
    validation::Validate,
};

pub struct NodeConfig {
    pub peer_reconnection_interval: Duration,
    pub max_backoff_interval: Duration,
    pub backoff_multiplier: f32,
    pub reconnection_retries_cap: u32,
    pub rpc_addr: Option<SocketAddr>,
    pub data_dir: PathBuf,
}

#[derive(Clone, Debug)]
struct PeerState {
    addr: SocketAddr,
    next_check: Instant,
    current_backoff: Duration,
    consecutive_failures: u32,
    capabilities: Capabilities,
}

impl PeerState {
    fn new(addr: SocketAddr, capabilities: Capabilities) -> Self {
        Self {
            addr,
            next_check: Instant::now(),
            current_backoff: Duration::from_secs(1),
            consecutive_failures: 0,
            capabilities,
        }
    }
}

impl std::fmt::Display for PeerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PeerState {{ addr: {}, next_check: {:?}, current_backoff: {:?}, consecutive_failures: {} }}",
            self.addr, self.next_check, self.current_backoff, self.consecutive_failures
        )
    }
}

pub struct Node {
    /// Peer ID
    peer_id: PeerId,
    /// Node listening address
    addr: SocketAddr,
    /// TCP listener
    listener: TcpListener,
    /// Bootstrap node
    bootstrap_node: Option<SocketAddr>,
    /// Current active peers
    connections: Arc<RwLock<HashMap<PeerId, (SocketAddr, Sender<Message>)>>>,
    /// Peers we know about
    known_peers: Arc<RwLock<HashMap<PeerId, PeerState>>>,
    /// Node capabilities
    capabilities: Capabilities,
    /// Node configuration
    config: NodeConfig,
}

#[derive(Serialize, Clone)]
struct NodeInfo {
    id: PeerId,
    peers: usize,
    connections: usize,
    listen_addr: SocketAddr,
    rpc_addr: Option<SocketAddr>,
}

impl IntoResponse for NodeInfo {
    type Output = NodeInfo;

    fn into_response(self) -> jsonrpsee::ResponsePayload<'static, Self::Output> {
        ResponsePayload::success(self)
    }
}

impl Node {
    pub async fn new(
        addr: SocketAddr,
        bootstrap_node: Option<SocketAddr>,
        config: NodeConfig,
    ) -> anyhow::Result<Arc<Self>> {
        let identity_manager = IdentityManager::new(config.data_dir.clone())?;

        // if node keypair exists at data-dir, load it, else generate a new one
        let keypair = identity_manager.load_or_generate_keypair()?;

        let peer_id = PeerId::from_public_key(&PublicKey::Ed25519(keypair.public()));
        event!(Level::INFO, "PeerID: {}", peer_id);

        // get node hardware capabilities
        let memory_refresh_kind = MemoryRefreshKind::nothing();
        let sys = System::new_with_specifics(
            RefreshKind::nothing()
                .with_memory(memory_refresh_kind.with_ram())
                .with_cpu(CpuRefreshKind::nothing()),
        );

        event!(Level::TRACE, "Loading Nvidia GPU capabilities if present");
        let mut nvidia_gpus = vec![];

        // load nvidia gpu data
        match Nvml::init() {
            Ok(nvml) => {
                for i in 0..nvml.device_count()? {
                    let device = nvml.device_by_index(i)?;
                    let card = GraphicCard {
                        id: device.uuid()?,
                        name: device.name()?,
                        brand: device.brand()?,
                        memory: device.memory_info()?.free,
                        architecture: device.architecture()?,
                        compute_mode: device.compute_mode()?,
                    };
                    nvidia_gpus.push(card);
                }
                event!(Level::TRACE, "Nvidia data loaded");
            }
            Err(e) => {
                event!(
                    Level::TRACE,
                    "No Nvidia GPUs found. Error: {}",
                    e.to_string()
                );
            }
        };

        Ok(Arc::new(Self {
            peer_id,
            addr,
            listener: TcpListener::bind(addr).await?,
            bootstrap_node,
            connections: Arc::new(RwLock::new(HashMap::new())),
            known_peers: Arc::new(RwLock::new(HashMap::new())),
            config,
            capabilities: Capabilities {
                cpu_cores: sys.cpus().len().clone(),
                memory: sys.total_memory() / 1_000_000_000,
                nvidia_gpus,
                supported_models: vec![],
            },
        }))
    }

    async fn get_node_info(&self) -> anyhow::Result<NodeInfo> {
        Ok(NodeInfo {
            id: self.peer_id,
            peers: self.known_peers.read().await.len(),
            connections: self.connections.read().await.len(),
            listen_addr: self.listener.local_addr()?,
            rpc_addr: self.config.rpc_addr,
        })
    }

    pub async fn start(self: Arc<Self>) -> anyhow::Result<!> {
        let _span = span!(Level::TRACE, "start").entered();
        if let Some(bootstrap_node) = self.bootstrap_node {
            event!(
                Level::INFO,
                "Connecting to bootstrap node at {}",
                &bootstrap_node
            );
            let this = self.clone();
            tokio::spawn(async move {
                let mut failed_peer_id = None;
                if let Err(err) = this
                    .connect_to_peer(&bootstrap_node, &mut failed_peer_id)
                    .await
                {
                    event!(Level::ERROR, "Error connecting to bootstrap node: {}", err);
                    if let Some(peer_id) = failed_peer_id {
                        this.connections.write().await.remove(&peer_id);
                    }
                }
            });
        }

        // Spawn a background task to run inactive known_peer reconnection
        let this = self.clone();
        tokio::spawn(async move { this.inactive_peer_reconnection().await });

        let this = self.clone();
        if let Some(rpc_addr) = self.config.rpc_addr {
            // Spawn a background task to run rpc server
            tokio::spawn(async move { this.start_rpc_server(rpc_addr).await });
        }

        // main loop to continuously listen for new tcp connections
        let this = self.clone();
        event!(Level::INFO, "Node listening on {}", this.addr);
        loop {
            let (socket_stream, _) = this.listener.accept().await?;

            let this = this.clone();
            // Spawns a new task for each incoming connection
            tokio::spawn(async move {
                // option peerId is passed up the call stack & set when peerId's decoded.
                // allows us to know what peerId to remove from connections
                let mut failed_peer_id = None;
                if let Err(err) = this
                    .handle_peer_connection(socket_stream, &mut failed_peer_id)
                    .await
                {
                    event!(Level::ERROR, "Error handling peer connection: {}", err);
                    if let Some(peer_id) = failed_peer_id {
                        this.connections.write().await.remove(&peer_id);
                    }
                }
            });
        }
    }

    async fn connect_to_peer(
        &self,
        peer_addr: &SocketAddr,
        peer_id: &mut Option<PeerId>,
    ) -> anyhow::Result<()> {
        let mut stream = TcpStream::connect(peer_addr).await?;
        event!(Level::INFO, "Connected to peer at {}", peer_addr);

        let message = Message::ConnectToPeerReq(ConnectionReq {
            peer_id: self.peer_id,
            listen_addr: self.addr,
            message: Some(format!("Sup peer at {}", peer_addr)),
            capabilities: self.capabilities.clone(),
        });

        let message_bytes = Self::serialize(message).await?;

        stream.write_all(&message_bytes).await?;

        self.handle_peer_connection(stream, peer_id).await?;

        Ok(())
    }

    async fn handle_peer_connection(
        &self,
        mut stream: TcpStream,
        peer_id: &mut Option<PeerId>,
    ) -> anyhow::Result<()> {
        // Channel for relaying msgs to internal message manager for forwarding to peers
        let (tx, mut rx): (Sender<Message>, Receiver<Message>) = mpsc::channel(16);

        // function to receive messages over peer's stream
        let recv = async |stream: &mut TcpStream| {
            let mut buffer = [0; 1024];

            let n = stream
                .read(&mut buffer)
                .await
                .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;

            if n == 0 {
                return Ok(None);
            }

            let decoded_slice: (Message, usize) =
                bincode::decode_from_slice(&buffer[..n], bincode::config::standard())?;

            anyhow::Ok(Some(decoded_slice))
        };

        // Configure heartbeat
        let mut heartbeat = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                res = recv(&mut stream) => {
                    if let Some(res) = res? {
                        let _ = self.handle_peer_message(res, tx.clone(), peer_id).await;
                    }
                }
                res = rx.recv() => {
                    if let Some(message) = res {
                        let _ = Self::send_message(&mut stream, message).await;
                    }
                }
                _ = heartbeat.tick() => {
                    // send keepalive ping message
                    let message = Message::Ping { timestamp_millis: SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64 };
                    let _ = Self::send_message(&mut stream, message).await;
                    event!(Level::TRACE, "Sent ping to peer");
                }
            }
        }
    }

    async fn handle_peer_message(
        &self,
        message: (Message, usize),
        tx: Sender<Message>,
        peer_id: &mut Option<PeerId>,
    ) -> anyhow::Result<()> {
        let bytes = message.1;
        match message.0 {
            Message::ConnectToPeerReq(connection_req) => {
                event!(
                    Level::INFO,
                    "Received {} bytes: \n{:#?}",
                    bytes,
                    connection_req
                );

                // update peer_id for call stack propagation
                // (handy in case of connection failure)
                *peer_id = Some(connection_req.peer_id);

                let peer_id = connection_req.peer_id;
                let peer_listen_addr = connection_req.listen_addr;

                // add to active connections map
                self.connections
                    .write()
                    .await
                    .insert(peer_id, (peer_listen_addr, tx.clone()));

                // add to known_peers if not already there
                let mut known_peers = self.known_peers.write().await;
                known_peers.entry(peer_id).or_insert(PeerState::new(
                    peer_listen_addr,
                    connection_req.capabilities,
                ));

                let response = Message::ConnectToPeerResp(ConnectionResp {
                    peer_id: self.peer_id,
                    listen_addr: self.addr,
                    known_peers: known_peers
                        .clone()
                        .iter()
                        .map(|(peer_id, peer_state)| {
                            (*peer_id, (peer_state.addr, peer_state.capabilities.clone()))
                        })
                        .collect(),
                    message: Some("Sup peer".to_string()),
                    capabilities: self.capabilities.clone(),
                });

                tx.send(response).await?;
            }
            Message::ConnectToPeerResp(mut connection_info) => {
                event!(
                    Level::INFO,
                    "Received {} bytes: \n{:#?}",
                    bytes,
                    connection_info
                );

                // update peer_id for call stack propagation
                *peer_id = Some(connection_info.peer_id);

                connection_info.known_peers.remove(&self.peer_id);

                // Extend known_peers with new peer & its known_peers
                let mut known_peers = self.known_peers.write().await;
                known_peers.extend(connection_info.known_peers.iter().map(
                    |(peer_id, (addr, capabilities))| {
                        (*peer_id, PeerState::new(*addr, capabilities.clone()))
                    },
                ));
                known_peers.insert(
                    connection_info.peer_id,
                    PeerState::new(connection_info.listen_addr, connection_info.capabilities),
                );

                // Extend connections with new peer's details
                self.connections.write().await.insert(
                    connection_info.peer_id,
                    (connection_info.listen_addr, tx.clone()),
                );
            }
            Message::Ping { timestamp_millis } => {
                event!(Level::TRACE, "Received ping from peer");
                tx.send(Message::Pong { timestamp_millis }).await?;
            }
            Message::Pong { timestamp_millis } => {
                let now_millis = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

                let latency_ms = now_millis.saturating_sub(timestamp_millis);
                event!(
                    Level::TRACE,
                    "Received pong from peer. Latency: {:?}ms",
                    latency_ms
                );
            }
            Message::TaskAnnouncement(task_announcement) => {
                task_announcement.validate()?;
            }
            Message::TaskClaim(task_claim) => {
                task_claim.validate()?;
            }
            Message::TaskResult(task_result) => {
                task_result.validate()?;
            }
        };

        Ok(())
    }

    async fn inactive_peer_reconnection(self: Arc<Self>) -> anyhow::Result<!> {
        let span = span!(Level::DEBUG, "peer_reconnection_loop");
        let _enter = span.enter();
        loop {
            event!(Level::INFO, "peer reconnection loop");

            // Sleep for the reconnection interval
            tokio::time::sleep(self.config.peer_reconnection_interval).await;

            let known_peers_hashset = self
                .known_peers
                .read()
                .await
                .clone()
                .into_keys()
                .collect::<HashSet<PeerId>>();

            let connections_hashset = self
                .connections
                .read()
                .await
                .clone()
                .into_keys()
                .collect::<HashSet<PeerId>>();

            let diff = known_peers_hashset.difference(&connections_hashset);

            for peer in diff.into_iter() {
                let this = self.clone();
                let peer = *peer;
                let mut known_peers = this.known_peers.write().await;

                if let Some(peer_state) = known_peers.get_mut(&peer) {
                    let peer_addr = peer_state.addr;
                    let this = this.clone();
                    let mut failed_peer_id = None;

                    // If peer reconnection retries exceeds cap, presume dead
                    // and remove from known_peers.
                    if peer_state.consecutive_failures >= this.config.reconnection_retries_cap {
                        continue;
                    }

                    if Instant::now() >= peer_state.next_check {
                        tokio::spawn(async move {
                            event!(
                                Level::INFO,
                                "attempting reconnection to peer {:?} on {}",
                                &peer,
                                &peer_addr
                            );
                            if let Err(err) =
                                this.connect_to_peer(&peer_addr, &mut failed_peer_id).await
                            {
                                if let Some(peer_id) = failed_peer_id {
                                    this.connections.write().await.remove(&peer_id);
                                }

                                // Update peer backoff
                                let mut known_peers = this.known_peers.write().await;

                                if let Some(peer_state) = known_peers.get_mut(&peer) {
                                    let new_backoff = (peer_state.current_backoff.as_secs_f32()
                                        * this.config.backoff_multiplier)
                                        .min(this.config.max_backoff_interval.as_secs_f32());

                                    peer_state.current_backoff =
                                        Duration::from_secs_f32(new_backoff);
                                    peer_state.next_check =
                                        Instant::now() + peer_state.current_backoff;
                                    peer_state.consecutive_failures += 1;

                                    event!(Level::ERROR,
                        "Reconnection attempt to peer: {} failed with error: {}, will retry in {:?}",
                        peer, err, peer_state.current_backoff);
                                }
                            }
                        });
                    }
                }
            }
        }
    }

    async fn start_rpc_server(self: Arc<Self>, addr: SocketAddr) -> anyhow::Result<()> {
        // let _span = span!(Level::DEBUG, "rpc-server").entered();
        event!(Level::INFO, "Starting RPC server");

        let server = Server::builder().build(addr).await?;
        let mut module = RpcModule::new(());

        // info RPC endpoint
        let this = self.clone();
        module.register_async_method("info", move |_, _, _| {
            let this = this.clone();
            async move {
                // return ID, peer count, connnection count
                let node_info: NodeInfo = this.get_node_info().await.unwrap();
                node_info
            }
        })?;

        // peers RPC endpoint
        let this = self.clone();
        module.register_async_method("peers", move |_, _, _| {
            let this = this.clone();
            async move {
                let peers = this
                    .known_peers
                    .read()
                    .await
                    .clone()
                    .into_keys()
                    .map(|p| p.to_string())
                    .collect::<Vec<String>>();

                peers
            }
        })?;

        // active_connections RPC endpoint
        let this = self.clone();
        module.register_async_method("connections", move |_, _, _| {
            let this = this.clone();
            async move {
                let connections = this
                    .connections
                    .read()
                    .await
                    .clone()
                    .into_keys()
                    .map(|p| p.to_string())
                    .collect::<Vec<String>>();

                connections
            }
        })?;

        let addrr = server.local_addr()?;
        let handle = server.start(module);
        event!(Level::INFO, "RPC server listening on {}", addrr);

        tokio::spawn(handle.stopped());
        Ok(())
    }

    async fn send_message(stream: &mut TcpStream, message: Message) -> anyhow::Result<()> {
        let bytes = Self::serialize(message).await?;
        let _ = stream.write(&bytes).await?;
        Ok(())
    }

    async fn serialize(message: Message) -> anyhow::Result<Vec<u8>> {
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(message, config)?;
        Ok(bytes)
    }
}
