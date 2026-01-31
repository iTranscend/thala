use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Cli {
    /// Address of node to connect to
    #[clap(short, long)]
    pub node_rpc: String,

    #[command(subcommand)]
    pub cmd: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Get node information
    Info,
    /// Get all known peers
    Peers,
    /// Get active connections
    Connections,
    /// Get node capabilities
    Capabilities,
}
