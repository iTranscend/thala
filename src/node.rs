use std::{
    collections::{HashMap, HashSet, VecDeque},
    net::SocketAddr,
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use jsonrpsee::server::{RpcModule, Server};
use litep2p::PeerId;
use litep2p::crypto::PublicKey;
use nvml_wrapper::Nvml;
use shared::{
    types::{Capabilities, GraphicCard, NodeInfo, Task, TaskId, TaskType},
    validation::Validate,
};
use sysinfo::{CpuRefreshKind, MemoryRefreshKind, RefreshKind, System};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{
        Mutex, RwLock,
        mpsc::{self, Receiver, Sender},
    },
};
use tracing::{Level, event, span};

use crate::{
    identity::IdentityManager,
    message::{ConnectionReq, ConnectionResp, Coordinator, Message, TaskAnnouncement, TaskClaim},
};

const MAX_MESSAGE_SIZE: usize = 1024 * 1024; // 1 MB

pub struct NodeConfig {
    pub peer_reconnection_interval: Duration,
    pub max_backoff_interval: Duration,
    pub backoff_multiplier: f32,
    pub reconnection_retries_cap: u32,
    pub rpc_addr: Option<SocketAddr>,
    pub data_dir: PathBuf,
    pub models: Vec<String>,
    pub datasets: Vec<String>,
}

#[derive(Clone, Debug)]
struct PeerInfo {
    addr: SocketAddr,
    /// Next timestamp at which to retry a failed connection
    next_check: Instant,
    /// Current downtime duration to wait before next check (needs review, next_check & current_backoff can be reduced to one struct entry)
    current_backoff: Duration,
    consecutive_failures: u32,
    capabilities: Capabilities,
}

impl PeerInfo {
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

impl std::fmt::Display for PeerInfo {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "PeerState {{ addr: {}, next_check: {:?}, current_backoff: {:?}, consecutive_failures: {} }}",
            self.addr, self.next_check, self.current_backoff, self.consecutive_failures
        )
    }
}

struct PendingConnectionsChannel {
    sender: Sender<SocketAddr>,
    receiver: Mutex<Receiver<SocketAddr>>,
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
    known_peers: Arc<RwLock<HashMap<PeerId, PeerInfo>>>,
    /// Node capabilities
    capabilities: Capabilities,
    /// Node configuration
    config: NodeConfig,
    /// Seen tasks
    seen_tasks: Arc<RwLock<HashSet<TaskId>>>,
    /// Pending tasks: received tasks that node node is capable of processing but is waiting for an ack to process
    pending_tasks: Arc<RwLock<HashMap<TaskId, Task>>>,
    /// Queue for tasks
    _task_queue: VecDeque<TaskId>,
    /// Channel for sending addresses of peers we are trying to connect to
    pending_connections_channel: PendingConnectionsChannel,
    /// Tracks which worker (PeerId) was assigned each task
    task_assignments: Arc<RwLock<HashMap<TaskId, PeerId>>>,
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

        // pending connections channel
        let (pending_tx, pending_rx) = mpsc::channel::<SocketAddr>(32);

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
            capabilities: Capabilities {
                cpu_cores: sys.cpus().len(),
                memory: sys.total_memory() / 1_000_000_000,
                nvidia_gpus,
                supported_models: config.models.clone(),
                supported_datasets: config.datasets.clone(),
            },
            config,
            seen_tasks: Arc::new(RwLock::new(HashSet::new())),
            pending_tasks: Arc::new(RwLock::new(HashMap::new())),
            _task_queue: VecDeque::new(),
            pending_connections_channel: PendingConnectionsChannel {
                sender: pending_tx,
                receiver: Mutex::new(pending_rx),
            },
            task_assignments: Arc::new(RwLock::new(HashMap::new())),
        }))
    }

    async fn get_node_info(&self) -> anyhow::Result<NodeInfo> {
        Ok(NodeInfo {
            id: self.peer_id,
            peers: self.known_peers.read().await.len(),
            connections: self.connections.read().await.len(),
            listen_addr: self.listener.local_addr()?,
            rpc_addr: self.config.rpc_addr,
            capabilities: self.capabilities.clone(),
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

        // Spawn a background task to sweep expired tasks from assignment and pending maps
        let this = self.clone();
        tokio::spawn(async move { this.expired_task_sweep().await });

        // Spawn a task to drain pending_connections
        let this = self.clone();
        tokio::spawn(async move {
            event!(Level::INFO, "Task spawned to drain pending connections");
            while let Some(addr) = this
                .pending_connections_channel
                .receiver
                .lock()
                .await
                .recv()
                .await
            {
                let this = this.clone();
                tokio::spawn(async move {
                    let mut failed_peer_id = None;
                    let _ = this.connect_to_peer(&addr, &mut failed_peer_id).await;
                });
            }
        });

        // main loop to continuously listen for new tcp connections
        let this = self.clone();
        event!(Level::INFO, "Node listening on {}", this.addr);
        loop {
            let (socket_stream, _) = this.listener.accept().await?;

            let this = this.clone();
            // Spawns a new task for each incoming connection
            tokio::spawn(async move {
                // Option<PeerId> is passed up the call stack & set when peerId's decoded.
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

        Self::send_message(&mut stream, message).await?;

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
            let mut len_buf = [0u8; 4];
            match stream.read_exact(&mut len_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
                Err(e) => return Err(anyhow::anyhow!("Read error: {}", e)),
            }

            let len = u32::from_be_bytes(len_buf) as usize;

            if len > MAX_MESSAGE_SIZE {
                return Err(anyhow::anyhow!(
                    "Message too large: {} bytes (max {})",
                    len,
                    MAX_MESSAGE_SIZE
                ));
            }

            let mut buf = vec![0u8; len];
            stream
                .read_exact(&mut buf)
                .await
                .map_err(|e| anyhow::anyhow!("Read error: {}", e))?;

            let message: Message = postcard::from_bytes(&buf)?;
            anyhow::Ok(Some((message, len)))
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
                known_peers
                    .entry(peer_id)
                    .or_insert(PeerInfo::new(peer_listen_addr, connection_req.capabilities));

                let response = Message::ConnectToPeerResp(ConnectionResp {
                    peer_id: self.peer_id,
                    listen_addr: self.addr,
                    known_peers: known_peers
                        .clone()
                        .iter()
                        .map(|(peer_id, peer_info)| {
                            (*peer_id, (peer_info.addr, peer_info.capabilities.clone()))
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
                        (*peer_id, PeerInfo::new(*addr, capabilities.clone()))
                    },
                ));
                known_peers.insert(
                    connection_info.peer_id,
                    PeerInfo::new(connection_info.listen_addr, connection_info.capabilities),
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
                event!(
                    Level::INFO,
                    "Received task announcement from peer {:#?}",
                    task_announcement
                );
                if let Err(e) = task_announcement.task.validate() {
                    event!(
                        Level::WARN,
                        "Rejecting invalid task {:?}: {}",
                        task_announcement.task.id(),
                        e
                    );
                    return Ok(());
                }

                // skip if already seen
                if !self
                    .seen_tasks
                    .write()
                    .await
                    .insert(task_announcement.task.id())
                {
                    return Ok(());
                }

                // check if node is capable
                if self.is_capable(&task_announcement.task) {
                    // send claim to coordinator
                    // TODO: if coordinator is msg sender, skip connections search and claim

                    // if a connection to coordinator does not exist, create one
                    let coordinator_peer_id = task_announcement.coordinator.peer_id;
                    let coordinator_addr = task_announcement.coordinator.addr;

                    let needs_connection = !self
                        .connections
                        .read()
                        .await
                        .contains_key(&coordinator_peer_id);

                    event!(
                        Level::INFO,
                        "needs_connection to coordinator: {}",
                        needs_connection
                    );

                    if needs_connection {
                        // spawn a new thread to connect to peer
                        self.pending_connections_channel
                            .sender
                            .send(coordinator_addr)
                            .await?;
                    }

                    // send claim
                    if let Some((_, tx)) = self.connections.read().await.get(&coordinator_peer_id) {
                        event!(Level::INFO, "coordinator connection exists");
                        event!(Level::INFO, "sending taskClaim to coordinator");
                        let msg = Message::TaskClaim(TaskClaim {
                            task_id: task_announcement.task.id(),
                            worker_id: self.peer_id,
                            estimated_duration: 90, // Todo: implement estimation
                        });
                        tx.send(msg).await?;
                    }

                    // add to pending
                    self.pending_tasks
                        .write()
                        .await
                        .insert(task_announcement.task.id(), task_announcement.task);

                    // TODO: rebroadcast
                }
            }
            Message::TaskClaim(task_claim) => {
                task_claim.validate()?;
                self.task_assignments
                    .write()
                    .await
                    .insert(task_claim.task_id, task_claim.worker_id);
                event!(
                    Level::INFO,
                    "Task {:?} assigned to worker {:?}",
                    task_claim.task_id,
                    task_claim.worker_id
                );
            }
            Message::TaskResult(task_result) => {
                task_result.validate()?;

                // Verify the sender is who they claim to be — the peer_id established
                // during the handshake must match the worker_id in the result.
                let sender_id = match peer_id {
                    Some(id) => *id,
                    None => {
                        event!(
                            Level::WARN,
                            "TaskResult received from peer with no established identity"
                        );
                        return Ok(());
                    }
                };
                if sender_id != task_result.worker_id {
                    event!(
                        Level::WARN,
                        "TaskResult sender {:?} does not match claimed worker_id {:?}",
                        sender_id,
                        task_result.worker_id
                    );
                    return Ok(());
                }

                let assigned = self
                    .task_assignments
                    .read()
                    .await
                    .get(&task_result.task_id)
                    .copied();
                match assigned {
                    Some(expected) if expected == task_result.worker_id => {
                        event!(
                            Level::INFO,
                            "TaskResult received from assigned worker {:?}",
                            task_result.worker_id
                        );
                        self.task_assignments
                            .write()
                            .await
                            .remove(&task_result.task_id);
                        self.pending_tasks
                            .write()
                            .await
                            .remove(&task_result.task_id);
                        // TODO: forward result to task submitter
                    }
                    Some(expected) => {
                        event!(
                            Level::WARN,
                            "TaskResult worker_id mismatch for task {:?}: expected {:?}, got {:?}",
                            task_result.task_id,
                            expected,
                            task_result.worker_id
                        );
                        return Ok(());
                    }
                    None => {
                        event!(
                            Level::WARN,
                            "TaskResult received for unknown/unassigned task {:?}",
                            task_result.task_id
                        );
                        return Ok(());
                    }
                }
            }
        };

        Ok(())
    }

    async fn expired_task_sweep(self: Arc<Self>) -> anyhow::Result<!> {
        loop {
            tokio::time::sleep(Duration::from_secs(60)).await;

            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs();

            let expired_ids: Vec<TaskId> = self
                .pending_tasks
                .read()
                .await
                .iter()
                .filter(|(_, task)| task.expires() <= now)
                .map(|(id, _)| *id)
                .collect();

            if !expired_ids.is_empty() {
                let mut pending = self.pending_tasks.write().await;
                let mut assignments = self.task_assignments.write().await;
                for id in &expired_ids {
                    pending.remove(id);
                    assignments.remove(id);
                }
                event!(
                    Level::INFO,
                    "Swept {} expired task(s) from pending and assignment maps",
                    expired_ids.len()
                );
            }
        }
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

            let diff = known_peers_hashset
                .difference(&connections_hashset)
                .copied()
                .collect::<Vec<_>>();

            self.clone().evict_or_reconnect_peers(diff).await;
        }
    }

    async fn evict_or_reconnect_peers(self: Arc<Self>, diff: Vec<PeerId>) {
        for peer in diff {
            let this = self.clone();
            let mut known_peers = this.known_peers.write().await;

            if let Some(peer_info) = known_peers.get_mut(&peer) {
                let peer_addr = peer_info.addr;
                let this = this.clone();
                let mut failed_peer_id = None;

                // If peer reconnection retries exceeds cap, presume dead
                // and remove from known_peers.
                if peer_info.consecutive_failures >= this.config.reconnection_retries_cap {
                    known_peers.remove(&peer);
                    continue;
                }

                if Instant::now() >= peer_info.next_check {
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

                            if let Some(peer_info) = known_peers.get_mut(&peer) {
                                let new_backoff = (peer_info.current_backoff.as_secs_f32()
                                    * this.config.backoff_multiplier)
                                    .min(this.config.max_backoff_interval.as_secs_f32());

                                peer_info.current_backoff = Duration::from_secs_f32(new_backoff);
                                peer_info.next_check = Instant::now() + peer_info.current_backoff;
                                peer_info.consecutive_failures += 1;

                                event!(
                                    Level::ERROR,
                                    "Reconnection attempt to peer: {} failed with error: {}, will retry in {:?}",
                                    peer,
                                    err,
                                    peer_info.current_backoff
                                );
                            }
                        }
                    });
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

        // capabilitites RPC endpoint
        let this = self.clone();
        module.register_async_method("capabilities", move |_, _, _| {
            let this = this.clone();
            async move { Some(this.capabilities.clone()) }
        })?;

        // broadcast_task RPC endpoint
        let this = self.clone();
        module.register_async_method("broadcast_task", move |params, _, _| {
            let task = params.one::<Task>().unwrap();

            let this = this.clone();
            async move {
                event!(Level::INFO, "Task received from client: {:?}", task.id());
                match this.broadcast_task(task).await {
                    Ok(_) => true,
                    Err(_) => false,
                }
            }
        })?;

        let addrr = server.local_addr()?;
        let handle = server.start(module);
        event!(Level::INFO, "RPC server listening on {}", addrr);

        tokio::spawn(handle.stopped());
        Ok(())
    }

    async fn broadcast_task(&self, task: Task) -> anyhow::Result<()> {
        let connections = self.connections.read().await.clone();

        event!(
            Level::INFO,
            "Broadcasting task {:?} to {} peer(s)",
            task.id(),
            connections.len()
        );

        let task_announcement = Message::TaskAnnouncement(TaskAnnouncement {
            task: task,
            coordinator: Coordinator {
                peer_id: self.peer_id,
                addr: self.addr,
            },
        });

        for connection in connections {
            let (_, (_, tx)) = connection;

            let _ = tx.send(task_announcement.clone()).await;
        }
        Ok(())
    }

    async fn send_message(stream: &mut TcpStream, message: Message) -> anyhow::Result<()> {
        let payload = postcard::to_stdvec(&message)?;
        let len = (payload.len() as u32).to_be_bytes();
        let mut framed = Vec::with_capacity(4 + payload.len());
        framed.extend_from_slice(&len);
        framed.extend_from_slice(&payload);
        stream.write_all(&framed).await?;
        Ok(())
    }

    fn is_capable(&self, task: &Task) -> bool {
        match task.kind() {
            TaskType::Benchmark { model, dataset } => {
                if self.capabilities.supported_models.contains(model)
                    && self.capabilities.supported_datasets.contains(dataset)
                {
                    return true;
                }
                false
            }
            TaskType::Training => todo!(),
            TaskType::Inference => todo!(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{SystemTime, UNIX_EPOCH};

    use litep2p::crypto::{PublicKey, ed25519::Keypair as Ed25519Keypair};
    use shared::types::{Capabilities, Task, TaskId, TaskType};

    use super::*;
    use crate::message::{Coordinator, TaskAnnouncement};

    fn test_capabilities() -> Capabilities {
        Capabilities {
            cpu_cores: 1,
            memory: 8,
            nvidia_gpus: vec![],
            supported_models: vec![],
            supported_datasets: vec![],
        }
    }

    fn test_config(dir_suffix: &str) -> NodeConfig {
        NodeConfig {
            peer_reconnection_interval: Duration::from_secs(3600),
            max_backoff_interval: Duration::from_secs(60),
            backoff_multiplier: 2.0,
            reconnection_retries_cap: 3,
            rpc_addr: None,
            data_dir: std::env::temp_dir().join(format!("thala_test_{}", dir_suffix)),
            models: vec![],
            datasets: vec![],
        }
    }

    #[tokio::test]
    async fn test_seen_tasks_dedup() {
        let node = Node::new(
            "127.0.0.1:0".parse().unwrap(),
            None,
            test_config("seen_tasks"),
        )
        .await
        .unwrap();

        let expires = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 120;
        let task = Task::new(
            TaskId::new(),
            TaskType::Benchmark {
                model: "unsupported".to_string(),
                dataset: "unsupported".to_string(),
            },
            expires,
        );
        let coordinator = Coordinator {
            peer_id: node.peer_id,
            addr: node.addr,
        };
        let msg = Message::TaskAnnouncement(TaskAnnouncement { task, coordinator });

        let (tx, _rx) = tokio::sync::mpsc::channel::<Message>(8);
        let mut peer_id = None;

        // First call — should insert into seen_tasks and process normally
        node.handle_peer_message((msg.clone(), 0), tx.clone(), &mut peer_id)
            .await
            .unwrap();
        assert_eq!(node.seen_tasks.read().await.len(), 1);

        // Second call — should return early; seen_tasks stays at 1
        node.handle_peer_message((msg.clone(), 0), tx.clone(), &mut peer_id)
            .await
            .unwrap();
        assert_eq!(node.seen_tasks.read().await.len(), 1);
    }

    #[tokio::test]
    async fn test_dead_peer_evicted() {
        let node = Node::new(
            "127.0.0.1:0".parse().unwrap(),
            None,
            test_config("dead_peer"),
        )
        .await
        .unwrap();

        let fake_peer_id =
            PeerId::from_public_key(&PublicKey::Ed25519(Ed25519Keypair::generate().public()));
        let peer_addr: SocketAddr = "127.0.0.1:19999".parse().unwrap();

        let mut peer_info = PeerInfo::new(peer_addr, test_capabilities());
        peer_info.consecutive_failures = 3; // equals reconnection_retries_cap

        node.known_peers
            .write()
            .await
            .insert(fake_peer_id, peer_info);
        assert_eq!(node.known_peers.read().await.len(), 1);

        node.clone()
            .evict_or_reconnect_peers(vec![fake_peer_id])
            .await;

        assert!(node.known_peers.read().await.is_empty());
    }
}
