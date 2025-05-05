#![feature(never_type)]

use std::{collections::HashSet, error::Error, net::SocketAddr, sync::Arc, time::Duration};

use clap::Parser;
use log::{error, info};
use message::{ConnectionInfo, Message};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::RwLock,
};

mod cli;
mod logger;
mod message;

struct Node {
    addr: SocketAddr,
    listener: TcpListener,
    bootstrap_node: Option<SocketAddr>,
    // Current active peers
    active_peers: Arc<RwLock<HashSet<SocketAddr>>>,
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
            active_peers: Arc::new(RwLock::new(HashSet::new())),
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

        let message = Message::ConnectToPeer(ConnectionInfo {
            listen_addr: self.addr,
            known_peers: self.known_peers.read().await.clone(),
            message: Some(format!("Sup peer at {}", peer_addr)),
        });

        let config = bincode::config::standard();
        let message_bytes = bincode::encode_to_vec(message, config)?;

        let _ = stream.write_all(&message_bytes).await?;

        Ok(())
    }

    async fn handle_peer_connection(
        self: Arc<Self>,
        mut socket_stream: TcpStream,
        addr: SocketAddr,
    ) {
        let mut buffer = [0; 1024];

        loop {
            let n = match socket_stream.read(&mut buffer).await {
                Ok(n) if n == 0 => break,
                Ok(n) => n,
                Err(err) => {
                    error!("Error reading from peer {}: {}", addr, err);
                    break;
                }
            };

            let decoded_slice: Message =
                match bincode::decode_from_slice(&buffer[..n], bincode::config::standard()) {
                    Ok((message, _)) => message,
                    Err(err) => {
                        error!(
                            "Error decoding message from peer when handling peer connection: {}",
                            err
                        );
                        break;
                    }
                };

            let message = match decoded_slice {
                Message::ConnectToPeer(connection_info) => connection_info,
            };

            info!("Received {} bytes from {}: {:#?}\n", n, addr, message);

            let peer_listen_addr = message.listen_addr;

            // add to active peers
            self.active_peers.write().await.insert(peer_listen_addr);

            // add to know_peers if not already there
            let mut known_peers = self.known_peers.write().await;
            if !known_peers.contains(&peer_listen_addr) {
                known_peers.insert(peer_listen_addr);
            }

            // TODO:Respond to peer for peer discovery
        }

        self.active_peers.write().await.remove(&addr);
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
