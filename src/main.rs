#![feature(never_type)]

use std::{error::Error, time::Duration};

use clap::Parser;
use tracing::{info, Level};

use node::{Node, NodeConfig};
use tracing_config::{init_tracing, TracingConfig};

mod cli;
mod error;
mod identity;
mod message;
mod node;
mod tracing_config;
mod types;
mod validation;

#[tokio::main]
async fn main() -> Result<!, Box<dyn Error>> {
    // Initialize tracing
    let tracing_config = TracingConfig {
        level: Level::INFO,
        json_output: false,
        enable_file_logging: true,
        file_path: Some("thala-node.log".to_string()),
        enable_metrics: false,
        ..Default::default()
    };
    init_tracing(tracing_config)?;

    info!("Starting Thala Node");

    let args = cli::Args::parse();

    let config = NodeConfig {
        peer_reconnection_interval: Duration::from_secs(args.peer_reconnection_interval),
        backoff_multiplier: args.backoff_multiplier,
        max_backoff_interval: Duration::from_secs(args.max_backoff_interval),
        reconnection_retries_cap: args.reconnection_retries_cap,
        rpc_addr: args.rpc_addr,
        data_dir: args.data_dir(),
    };

    let node = Node::new(args.listen_address, args.bootstrap_node, config).await?;
    node.start().await?;
}
