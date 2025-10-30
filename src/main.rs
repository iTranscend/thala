#![feature(never_type)]

use std::{error::Error, time::Duration};

use clap::Parser;

use node::{Node, NodeConfig};

mod cli;
mod logger;
mod message;
mod node;

async fn init() {
    logger::setup();
}

#[tokio::main]
async fn main() -> Result<!, Box<dyn Error>> {
    init().await;
    let args = cli::Args::parse();

    let config = NodeConfig {
        peer_reconnection_interval: Duration::from_secs(args.peer_reconnection_interval),
    };

    let node = Node::new(args.listen_address, args.bootstrap_node, config).await?;
    node.start().await?;
}
