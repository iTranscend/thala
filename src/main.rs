#![feature(never_type)]

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use clap::Parser;
use log::{error, info};
use tokio::{
    io::{AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::RwLock,
};

mod cli;
mod logger;

struct Node {
    addr: SocketAddr,
    listener: TcpListener,
    bootstrap_node: Option<SocketAddr>,
    // Current active peers
    peers: Arc<RwLock<HashSet<SocketAddr>>>,
    // Peers we know about
    known_peers: Arc<RwLock<HashSet<SocketAddr>>>,
    config: NodeConfig,
}

struct NodeConfig {
    max_peers: usize,
    connection_timeout: Duration,
}

impl Node {
    async fn new(
        addr: SocketAddr,
        bootstrap_node: Option<SocketAddr>,
        config: NodeConfig,
    ) -> anyhow::Result<Arc<Self>> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Arc::new(Self {
            addr,
            listener,
            bootstrap_node,
            peers: Arc::new(RwLock::new(HashSet::new())),
            known_peers: Arc::new(RwLock::new(HashSet::new())),
            config,
        }))
    }

    async fn start(self: Arc<Self>) -> anyhow::Result<!> {
        if let Some(bootstrap_node) = self.bootstrap_node {
            info!("Connecting to bootstrap node at {}", &bootstrap_node);
            self.connect_to_peer(&bootstrap_node).await?;
        }

        info!("Node listening on {}", self.addr);
        // Spawns a new task for each incoming connection
        loop {
            let (socket_stream, addr) = self.listener.accept().await?;

            let this = self.clone();
            tokio::spawn(async move {
                this.handle_peer_connection(socket_stream, addr).await;
            });
        }
    }

    async fn connect_to_peer(&self, peer_addr: &SocketAddr) -> anyhow::Result<()> {
        let mut stream = TcpStream::connect(peer_addr).await?;
        info!("Connected to peer at {}", peer_addr);

        let message = format!("Sup peer at {}", peer_addr);
        stream.write_all(message.as_bytes()).await?;

        let mut buffer = [0; 1024];
        let n = stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        info!(
            "Received {} bytes from peer {} translated to : {}",
            n, peer_addr, response
        );

        Ok(())
    }

    async fn handle_peer_connection(
        self: Arc<Self>,
        mut socket_stream: TcpStream,
        addr: SocketAddr,
        peers: Arc<RwLock<HashSet<SocketAddr>>>,
        known_peers: Arc<RwLock<HashSet<SocketAddr>>>,
    ) {
        peers.write().await.insert(addr);

        // add to knonw_peers if not already there
        let mut known_peers = known_peers.write().await;
        if !known_peers.contains(&addr) {
            known_peers.insert(addr);
        }

        let mut buffer = [0; 1024];

        loop {
            let n = match socket_stream.read(&mut buffer).await {
                Ok(n) if n == 0 => break,
                Ok(n) => {
                    info!("Received {} BYTES\n", n);
                    n
                }
                Err(err) => {
                    error!("Error reading from peer {}: {}", addr, err);
                    break;
                }
            };

            let message = String::from_utf8_lossy(&buffer[..n]);
            info!("Received {} bytes from {}: {}\n", n, addr, message);

            let response = format!("Received message. Hello, peer at {}\n", addr);
            match socket_stream.write_all(response.as_bytes()).await {
                Ok(_) => {}
                Err(err) => {
                    error!("Error writing to peer {}: {}", addr, err);
                    break;
                }
            }
        }

        peers.write().await.remove(&addr);
    }
}

async fn init() {
    logger::setup();
}

#[tokio::main]
async fn main() -> Result<!, Box<dyn Error>> {
    init().await;
    let args = cli::Args::parse();

    let config = NodeConfig {
        max_peers: args.max_peers,
        connection_timeout: Duration::from_secs(args.connection_timeout),
    };

    let node = Node::new(args.listen_address, args.bootstrap_node, config).await?;
    node.start().await?;
}
