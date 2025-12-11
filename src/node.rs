use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use jsonrpsee::server::{RpcModule, Server};
use jsonrpsee::{core::Serialize, IntoResponse, ResponsePayload};
use litep2p::crypto::{ed25519::Keypair, PublicKey};
use litep2p::PeerId;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{
        mpsc::{self, Receiver, Sender},
        RwLock,
    },
};
use tracing::{event, span, Level};

use crate::message::{ConnectionReq, ConnectionResp, Message};

pub struct NodeConfig {
    pub peer_reconnection_interval: Duration,
    pub max_backoff_interval: Duration,
    pub backoff_multiplier: f32,
    pub reconnection_retries_cap: u32,
    pub rpc_addr: Option<SocketAddr>,
}

#[derive(Clone, Debug)]
struct PeerState {
    addr: SocketAddr,
    next_check: Instant,
    current_backoff: Duration,
    consecutive_failures: u32,
}

impl PeerState {
    fn new(addr: SocketAddr) -> Self {
        Self {
            addr,
            next_check: Instant::now(),
            current_backoff: Duration::from_secs(1),
            consecutive_failures: 0,
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
    /// Node configuration
    config: NodeConfig,
    /// The last time inactive peer reconnection was attempted
    last_peer_reconnection_timestamp: Arc<RwLock<Instant>>,
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
        let listener = TcpListener::bind(addr).await?;
        let keypair = Keypair::generate();
        let peer_id = PeerId::from_public_key(&PublicKey::Ed25519(keypair.public()));
        event!(Level::INFO, "PeerID: {}", peer_id);

        Ok(Arc::new(Self {
            peer_id,
            addr,
            listener,
            bootstrap_node,
            connections: Arc::new(RwLock::new(HashMap::new())),
            known_peers: Arc::new(RwLock::new(HashMap::new())),
            config,
            last_peer_reconnection_timestamp: Arc::new(RwLock::new(Instant::now())),
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
        tokio::spawn(async move { Self::inactive_peer_reconnection(this) });

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

            let n = tokio::time::timeout(Duration::from_secs(20), stream.read(&mut buffer))
                .await
                .map_err(|_| anyhow::anyhow!("Read timeout"))?
                .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;

            if n == 0 {
                return Ok(None);
            }

            let decoded_slice: (Message, usize) =
                bincode::decode_from_slice(&buffer[..n], bincode::config::standard())?;

            anyhow::Ok(Some(decoded_slice))
        };

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
                known_peers
                    .entry(peer_id)
                    .or_insert(PeerState::new(peer_listen_addr));

                let response = Message::ConnectToPeerResp(ConnectionResp {
                    peer_id: self.peer_id,
                    listen_addr: self.addr,
                    known_peers: known_peers
                        .clone()
                        .iter()
                        .map(|(peer_id, peer_state)| (*peer_id, peer_state.addr))
                        .collect(),
                    message: Some("Sup peer".to_string()),
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
                known_peers.extend(
                    connection_info
                        .known_peers
                        .iter()
                        .map(|(peer_id, addr)| (*peer_id, PeerState::new(*addr))),
                );
                known_peers.insert(
                    connection_info.peer_id,
                    PeerState::new(connection_info.listen_addr),
                );

                // Extend connections with new peer's details
                self.connections.write().await.insert(
                    connection_info.peer_id,
                    (connection_info.listen_addr, tx.clone()),
                );
            }
        };

        Ok(())
    }

    async fn inactive_peer_reconnection(this: Arc<Node>) -> anyhow::Result<!> {
        let span = span!(Level::DEBUG, "peer_reconnection_loop");
        let _enter = span.enter();
        loop {
            if { this.last_peer_reconnection_timestamp.read().await }.elapsed()
                >= this.config.peer_reconnection_interval
            {
                let known_peers_hashset = this
                    .known_peers
                    .read()
                    .await
                    .clone()
                    .into_keys()
                    .collect::<HashSet<PeerId>>();

                let connections_hashset = this
                    .connections
                    .read()
                    .await
                    .clone()
                    .into_keys()
                    .collect::<HashSet<PeerId>>();

                let diff = known_peers_hashset.difference(&connections_hashset);

                for peer in diff.into_iter() {
                    let this = this.clone();
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
                                        let new_backoff = (peer_state
                                            .current_backoff
                                            .as_secs_f32()
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

                let mut last_peer_reconnection_timestamp =
                    this.last_peer_reconnection_timestamp.write().await;
                *last_peer_reconnection_timestamp = Instant::now();
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
