#![feature(never_type)]

use std::{
    collections::{HashMap, HashSet},
    error::Error,
    net::SocketAddr,
    time::Duration,
};

use clap::Parser;
use log::{error, info};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

mod cli;

struct Node {
    addr: SocketAddr,
    listener: TcpListener,
    bootstrap_node: Option<SocketAddr>,
    // Current active peers
    peers: HashMap<SocketAddr, TcpStream>,
    // Peers we know about
    known_peers: HashSet<SocketAddr>,
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
    ) -> anyhow::Result<Self> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self {
            addr,
            listener,
            bootstrap_node,
            peers: HashMap::new(),
            known_peers: HashSet::new(),
            config,
        })
    }

    async fn start(&self) -> anyhow::Result<!> {
        if let Some(bootstrap_node) = self.bootstrap_node {
            info!("Connecting to bootstrap node at {}", &bootstrap_node);
            self.connect_to_peer(&bootstrap_node).await?;
        }

        info!("Node listening on {}", self.addr);
        // Spawns a new task for each incoming connection
        loop {
            let (socket_stream, addr) = self.listener.accept().await?;

            tokio::spawn(Self::handle_peer_connection(socket_stream, addr));
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

    async fn handle_peer_connection(mut socket_stream: TcpStream, addr: SocketAddr) {
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
    }
}

async fn init() {
    env_logger::init();
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
