use std::error::Error;

use clap::Parser;
use jsonrpsee::{
    core::client::ClientT,
    http_client::{HeaderMap, HeaderValue, HttpClient},
    rpc_params,
};
use shared::tracing::{TracingConfig, init_tracing};
use shared::types::{Capabilities, NodeInfo};
use tracing::{Level, event};

use crate::cli::Cli;

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

    let cli = Cli::parse();
    let rpc = cli.node_rpc;

    let mut headers = HeaderMap::new();
    headers.insert("Content-Type", HeaderValue::from_static("application/json"));
    let client = HttpClient::builder()
        .set_headers(headers)
        .build(rpc.to_string())?;

    match cli.cmd {
        cli::Commands::Info => {
            event!(Level::DEBUG, "info RPC call made to {:?}", &rpc);
            let res: NodeInfo = client.request("info", rpc_params!("")).await?;
            println!("{:#?}", res);
        }
        cli::Commands::Peers => {
            event!(Level::DEBUG, "peers RPC call made to {:?}", &rpc);
            let res: Vec<String> = client.request("peers", rpc_params!("")).await?;
            println!("{:#?}", res);
        }
        cli::Commands::Connections => {
            event!(
                Level::DEBUG,
                "active_connections RPC call made to {:?}",
                &rpc
            );
            let res: Vec<String> = client.request("connections", rpc_params!("")).await?;
            println!("{:#?}", res);
        }
        cli::Commands::Capabilities => {
            event!(Level::DEBUG, "capabilities RPC call made to {:?}", &rpc);
            let res: Capabilities = client.request("capabilities", rpc_params!("")).await?;
            println!("{:#?}", res);
        }
    }

    Ok(())
}
