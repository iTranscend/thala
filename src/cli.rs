use std::net::SocketAddr;

use clap::Parser;

#[derive(Parser)]
pub struct Args {
    #[clap(short, long, default_value = "127.0.0.1:2345")]
    pub listen_address: SocketAddr,

    #[clap(short, long)]
    pub bootstrap_node: Option<SocketAddr>,
}
