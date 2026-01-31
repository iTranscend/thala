use clap::{Parser, Subcommand};

#[derive(Parser)]
pub struct Cli {
    /// Address of node to connect to
    #[clap(short, long)]
    pub node_rpc: String,

    #[command(subcommand)]
    pub cmd: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Get node information
    Info,
    /// Get all known peers
    Peers,
    /// Get active connections
    Connections,
    /// Get node capabilities
    Capabilities,
    /// Create task and send to network
    #[command(subcommand)]
    Task(Task),
}

#[derive(Subcommand)]
pub enum Task {
    /// Run benchmark
    Benchmark(Benchmark),
}

#[derive(Parser)]
pub struct Benchmark {
    /// AI model
    #[clap(short, long)]
    pub model: String,
    /// Dataset to benchmark model against
    #[clap(short, long)]
    pub dataset: String,
}
