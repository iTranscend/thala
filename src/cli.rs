use std::{net::SocketAddr, path::PathBuf};

use clap::Parser;

#[derive(Parser)]
pub struct Args {
    /// Address and port to listen on for new connection requests
    #[clap(short, long, default_value = "127.0.0.1:2345")]
    pub listen_address: SocketAddr,

    /// Boostrap node address
    #[clap(short, long)]
    pub bootstrap_node: Option<SocketAddr>,

    /// Time interval in secs to attempt peer reconnection
    #[clap(short, long, default_value = "30")]
    pub peer_reconnection_interval: u64,

    /// Multiplying factor for exponential backoff
    #[clap(short = 'x', long, default_value = "2.0")]
    pub backoff_multiplier: f32,

    /// Maximum backoff interval in secs
    #[clap(short, long, default_value = "2000")]
    pub max_backoff_interval: u64,

    /// Reconnection retries at which peer reconnection attempts should be stopped
    #[clap(short, long, default_value = "13")]
    pub reconnection_retries_cap: u32,

    /// RPC listening address
    #[clap(long)]
    pub rpc_addr: Option<SocketAddr>,

    /// Data directory path (default: ~/.thala)
    #[clap(short, long)]
    pub data_dir: Option<PathBuf>,
}

impl Args {
    pub fn data_dir(&self) -> PathBuf {
        self.data_dir.clone().unwrap_or_else(|| {
            dirs::home_dir()
                .expect("Could not determine home directory")
                .join(".thala")
        })
    }
}
