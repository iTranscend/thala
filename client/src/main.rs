use std::error::Error;

use clap::Parser;
use jsonrpsee::{
    core::client::ClientT,
    http_client::{HeaderMap, HeaderValue, HttpClient},
    rpc_params,
};
use shared::types::{Capabilities, NodeInfo, Task};
use shared::{
    tracing::{TracingConfig, init_tracing},
    types::{TaskId, TaskType},
};
use tracing::{Level, event};

mod cli;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    println!("thala client");

    // Initialize tracing
    let tracing_config = TracingConfig {
        level: Level::INFO,
        json_output: false,
        enable_file_logging: false,
        file_path: None,
        enable_metrics: false,
        ..Default::default()
    };
    init_tracing(tracing_config)?;

    let cli = cli::Cli::parse();
    let rpc = cli.node_rpc;

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    let client = HttpClient::builder()
        .set_headers(headers)
        .build(rpc.to_string())?;

    match cli.cmd {
        cli::Command::Info => {
            event!(Level::DEBUG, "info RPC call made to {:?}", &rpc);
            let res: NodeInfo = client.request("info", rpc_params!("")).await?;
            println!("{:#?}", res);
        }
        cli::Command::Peers => {
            event!(Level::DEBUG, "peers RPC call made to {:?}", &rpc);
            let res: Vec<String> = client.request("peers", rpc_params!("")).await?;
            println!("{:#?}", res);
        }
        cli::Command::Connections => {
            event!(
                Level::DEBUG,
                "active_connections RPC call made to {:?}",
                &rpc
            );
            let res: Vec<String> = client.request("connections", rpc_params!("")).await?;
            println!("{:#?}", res);
        }
        cli::Command::Capabilities => {
            event!(Level::DEBUG, "capabilities RPC call made to {:?}", &rpc);
            let res: Capabilities = client.request("capabilities", rpc_params!("")).await?;
            println!("{:#?}", res);
        }
        cli::Command::Task(task) => match task {
            cli::Task::Benchmark(benchmark) => {
                let model = benchmark.model;
                let dataset = benchmark.dataset;

                let task_type = TaskType::Benchmark { model, dataset };
                let task = Task::new(TaskId::new(), task_type);
                println!("New Benchmark task created: {:?}", task.id());

                event!(Level::DEBUG, "broadcast_task RPC call made to {:?}", &rpc);
                let res: bool = client.request("broadcast_task", rpc_params!(task)).await?;
                println!("{:#?}", res);
            }
        },
    }

    Ok(())
}
