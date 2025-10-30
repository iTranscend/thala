use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use log::{error, info};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{
        mpsc::{self, Receiver, Sender},
        RwLock,
    },
};

use crate::message::{ConnectionReq, ConnectionResp, Message};

pub struct NodeConfig {
    pub peer_reconnection_interval: Duration,
}

pub struct Node {
    addr: SocketAddr,
    listener: TcpListener,
    bootstrap_node: Option<SocketAddr>,
    /// Current active peers
    // TODO: key should be peer id (or not)
    connections: Arc<RwLock<HashMap<SocketAddr, Sender<Message>>>>,
    /// Peers we know about
    known_peers: Arc<RwLock<HashSet<SocketAddr>>>,
    /// Node configuration
    config: NodeConfig,
    /// The last time inactive peer reconnection was attempted
    last_peer_reconnection_timestamp: Arc<RwLock<Instant>>,
}

impl Node {
    pub async fn new(
        addr: SocketAddr,
        bootstrap_node: Option<SocketAddr>,
        config: NodeConfig,
    ) -> anyhow::Result<Arc<Self>> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Arc::new(Self {
            addr,
            listener,
            bootstrap_node,
            connections: Arc::new(RwLock::new(HashMap::new())),
            known_peers: Arc::new(RwLock::new(HashSet::new())),
            config,
            last_peer_reconnection_timestamp: Arc::new(RwLock::new(Instant::now())),
        }))
    }

    pub async fn start(self: Arc<Self>) -> anyhow::Result<!> {
        if let Some(bootstrap_node) = self.bootstrap_node {
            info!("Connecting to bootstrap node at {}", &bootstrap_node);
            let this = self.clone();
            tokio::spawn(async move {
                if let Err(err) = this.connect_to_peer(&bootstrap_node).await {
                    error!("Error connecting to bootstrap node: {}", err);
                }
            });
        }

        // Spawn a background task to run inactive known_peer reconnnection
        let this = self.clone();
        tokio::spawn(async move {
            loop {
                if this.last_peer_reconnection_timestamp.read().await.elapsed()
                    >= this.config.peer_reconnection_interval
                {
                    tokio::time::sleep(this.config.peer_reconnection_interval).await;

                    let known_peers = this.known_peers.read().await;
                    let connections_hashset = this
                        .connections
                        .read()
                        .await
                        .clone()
                        .into_keys()
                        .collect::<HashSet<SocketAddr>>();

                    let diff = known_peers.difference(&connections_hashset);

                    for peer in diff.into_iter() {
                        let peer = peer.clone();
                        let this = this.clone();
                        tokio::spawn(async move {
                            info!("attempting reconnection to peer {:?}", &peer);
                            if let Err(err) = this.connect_to_peer(&peer).await {
                                error!(
                            "Reconnection attempt to peer: {:?} failed with error: {:?}, will retry in  seconds",
                            peer, err
                        );
                            }
                        });
                    }

                    let mut last_peer_reconnection_timestamp =
                        this.last_peer_reconnection_timestamp.write().await;
                    *last_peer_reconnection_timestamp = Instant::now();
                }
            }
        });

        // main loop to continuously listen for new tcp connections
        let this = self.clone();
        info!("Node listening on {}", this.addr);
        loop {
            let (socket_stream, addr) = this.listener.accept().await?;

            let this = this.clone();
            // Spawns a new task for each incoming connection
            tokio::spawn(async move {
                if let Err(err) = this.handle_peer_connection(socket_stream).await {
                    error!("Error handling peer connection: {}", err);
                    // TODO: (fix) need to remove peer's inbound address from connections
                    // not outbound addr, peer's outbound addr is not stored
                    this.connections.write().await.remove(&addr);
                    // this.connections.write().await.drain();
                }
            });
        }
    }

    async fn connect_to_peer(&self, peer_addr: &SocketAddr) -> anyhow::Result<()> {
        let mut stream = TcpStream::connect(peer_addr).await?;
        info!("Connected to peer at {}", peer_addr);

        let message = Message::ConnectToPeerReq(ConnectionReq {
            listen_addr: self.addr,
            message: Some(format!("Sup peer at {}", peer_addr)),
        });

        let message_bytes = Self::serialize(message).await?;

        stream.write_all(&message_bytes).await?;

        self.handle_peer_connection(stream).await?;

        Ok(())
    }

    async fn handle_peer_connection(&self, mut stream: TcpStream) -> anyhow::Result<()> {
        // Channel for relaying msgs to internal message manager for forwarding to peers
        let (tx, mut rx): (Sender<Message>, Receiver<Message>) = mpsc::channel(16);

        // function to receive messages over peer's stream
        let recv = async |stream: &mut TcpStream| {
            let mut buffer = [0; 1024];

            let n = stream.read(&mut buffer).await?;

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
                        let _ = self.handle_peer_message(res, tx.clone()).await;
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
    ) -> anyhow::Result<()> {
        let bytes = message.1;
        match message.0 {
            Message::ConnectToPeerReq(connection_req) => {
                info!("Received {} bytes: \n{:#?}", bytes, connection_req);

                let peer_listen_addr = connection_req.listen_addr;

                // add to active connections map
                self.connections
                    .write()
                    .await
                    .insert(peer_listen_addr, tx.clone());

                // add to known_peers if not already there
                let mut known_peers = self.known_peers.write().await;
                if !known_peers.contains(&peer_listen_addr) {
                    known_peers.insert(peer_listen_addr);
                }

                let response = Message::ConnectToPeerResp(ConnectionResp {
                    listen_addr: self.addr,
                    known_peers: known_peers.clone(),
                    message: Some(format!("Sup peer")),
                });

                let _ = tx.send(response).await?;
            }
            Message::ConnectToPeerResp(connection_info) => {
                info!("Received {} bytes: \n{:#?}", bytes, connection_info);

                // Extend known_peers with new peer's known_peers
                let mut known_peers = self.known_peers.write().await;
                known_peers.extend(connection_info.known_peers);
            }
        };

        Ok(())
    }

    async fn send_message(stream: &mut TcpStream, message: Message) -> anyhow::Result<()> {
        let bytes = Self::serialize(message).await?;
        stream.write(&bytes).await?;
        Ok(())
    }

    async fn serialize(message: Message) -> anyhow::Result<Vec<u8>> {
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(message, config)?;
        Ok(bytes)
    }
}
