use std::{
    collections::{HashMap, HashSet},
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use log::info;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{
        mpsc::{self, Receiver, Sender},
        Mutex, RwLock,
    },
};

use crate::message::{ConnectionInfo, Message};

pub struct NodeConfig {
    pub max_peers: usize,
    pub connection_timeout: Duration,
}

pub struct Node {
    addr: SocketAddr,
    listener: TcpListener,
    bootstrap_node: Option<SocketAddr>,
    // Current active peers
    connections: Arc<RwLock<HashMap<SocketAddr, Arc<Mutex<TcpStream>>>>>,
    // Peers we know about
    known_peers: Arc<RwLock<HashSet<SocketAddr>>>,
    config: NodeConfig,
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
        }))
    }

    pub async fn start(self: Arc<Self>) -> anyhow::Result<!> {
        if let Some(bootstrap_node) = self.bootstrap_node {
            info!("Connecting to bootstrap node at {}", &bootstrap_node);
            self.connect_to_peer(&bootstrap_node).await?;
        }

        info!("Node listening on {}", self.addr);
        // Spawns a new task for each incoming connection
        loop {
            let (socket_stream, _) = self.listener.accept().await?;

            let this = self.clone();
            tokio::spawn(async move {
                let _ = this.handle_peer_connection(socket_stream).await;
            });
        }
    }

    async fn connect_to_peer(&self, peer_addr: &SocketAddr) -> anyhow::Result<()> {
        let mut stream = TcpStream::connect(peer_addr).await?;
        info!("Connected to peer at {}", peer_addr);

        let message = Message::ConnectToPeer(ConnectionInfo {
            listen_addr: self.addr,
            known_peers: self.known_peers.read().await.clone(),
            message: Some(format!("Sup peer at {}", peer_addr)),
        });

        let message_bytes = Self::serialize(message).await?;

        stream.write_all(&message_bytes).await?;

        let _ = stream.write_all(&message_bytes).await?;

        Ok(())
    }

    async fn handle_peer_connection(self: Arc<Self>, stream: TcpStream) -> anyhow::Result<()> {
        // create channel for sending msgs between peer tasks
        let (tx, mut rx): (
            Sender<(Message, Arc<Mutex<TcpStream>>)>,
            Receiver<(Message, Arc<Mutex<TcpStream>>)>,
        ) = mpsc::channel(16);

        let stream = Arc::new(Mutex::new(stream));

        loop {
            // TODO: use unsafe for TcpStream handling between sender and receiver tasks since we know only one of the tasks will execute
            let tx = tx.clone();
            let task = async {
                let mut buffer = [0; 1024];

                let n = stream.lock().await.read(&mut buffer).await?;

                if n == 0 {
                    return Ok::<Option<(Message, usize)>, anyhow::Error>(None);
                }

                let decoded_slice: (Message, usize) =
                    bincode::decode_from_slice(&buffer[..n], bincode::config::standard())?;
                Ok(Some(decoded_slice))
            };

            tokio::select! {
                res = task => {
                    let res = res?;
                    if let Some(res) = res {
                        let _ = self.handle_peer_message(stream.clone(), res, tx).await;
                    }
                }
                res = rx.recv() => {
                    if let Some((message, stream)) = res {
                        let bytes = Self::serialize(message).await?;
                        stream.lock().await.write(&bytes).await?;
                    }
                }
            }
        }

        // self.connections.write().await.remove(&addr);
        // Ok(())
    }

    async fn handle_peer_message(
        &self,
        stream: Arc<Mutex<TcpStream>>,
        message: (Message, usize),
        tx: Sender<(Message, Arc<Mutex<TcpStream>>)>,
    ) -> anyhow::Result<()> {
        let bytes = message.1;
        let message = match message.0 {
            Message::ConnectToPeer(connection_info) => connection_info,
        };

        info!("Received {} bytes {:#?}\n", bytes, message);

        let peer_listen_addr = message.listen_addr;

        // add to active connections map
        self.connections
            .write()
            .await
            .insert(peer_listen_addr, stream.clone());

        // add to known_peers if not already there
        let mut known_peers = self.known_peers.write().await;
        if !known_peers.contains(&peer_listen_addr) {
            known_peers.insert(peer_listen_addr);
        }

        info!("Respond to peer");

        // TODO:Respond to peer for peer discovery
        let response = Message::ConnectToPeer(ConnectionInfo {
            listen_addr: self.addr,
            known_peers: known_peers.clone(),
            message: Some(format!("Sup peer")),
        });

        self.send_message(tx, response, stream).await;

        Ok(())
    }

    async fn send_message(
        &self,
        tx: Sender<(Message, Arc<Mutex<TcpStream>>)>,
        message: Message,
        stream: Arc<Mutex<TcpStream>>,
    ) {
        let _ = tx.send((message, stream)).await;
    }

    async fn serialize(message: Message) -> anyhow::Result<Vec<u8>> {
        let config = bincode::config::standard();
        let bytes = bincode::encode_to_vec(message, config)?;
        Ok(bytes)
    }
}
