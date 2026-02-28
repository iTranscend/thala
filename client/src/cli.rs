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

#[derive(Parser)]
pub struct TaskOptions {
    /// Task expiration in seconds from now (default: 3600)
    #[clap(short, long, default_value = "3600")]
    pub expires: u64,
}

#[derive(Subcommand)]
pub enum Task {
    /// Run benchmark
    Benchmark(Benchmark),
}

#[derive(Parser)]
pub struct Benchmark {
    #[clap(flatten)]
    pub options: TaskOptions,
    /// AI model
    #[clap(short, long)]
    pub model: String,
    /// Dataset to benchmark model against
    #[clap(short, long)]
    pub dataset: String,
}
