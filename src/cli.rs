use std::net::SocketAddr;

use clap::Parser;

#[derive(Parser)]
pub struct Args {
    #[clap(short, long, default_value = "127.0.0.1:2345")]
    pub listen_address: SocketAddr,

    #[clap(short, long)]
    pub bootstrap_node: Option<SocketAddr>,

    #[clap(short, long, default_value = "30")]
    pub peer_reconnection_interval: u64,

    #[clap(short = 'x', long, default_value = "2.0")]
    pub backoff_multiplier: f32,

    #[clap(short, long, default_value = "2000")]
    pub max_backoff_interval: u64,

    /// Reconnection retries at which the node should be considered dead
    #[clap(short, long, default_value = "13")]
    pub reconnection_retries_cap: u32,
}
