#![feature(never_type)]

use std::error::Error;

use clap::Parser;
use log::{error, info};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

mod cli;

async fn init() {
    env_logger::init();
}

#[tokio::main]
async fn main() -> Result<!, Box<dyn Error>> {
    init().await;
    let args = cli::Args::parse();

    if let Some(bootstrap_node) = args.bootstrap_node {
        info!("Connecting to bootstrap node at {}", bootstrap_node);

        let mut stream = TcpStream::connect(bootstrap_node).await?;
        info!("Connected to bootstrap node!");

        let message = "Sup bootstrap node";
        stream.write_all(message.as_bytes()).await?;

        let mut buffer = [0; 1024];
        let n = stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        info!(
            "Received {} bytes from bootstrap node {} translated to : {}",
            n, bootstrap_node, response
        );
    }

    let listener = TcpListener::bind(args.listen_address).await?;
    info!("Node listening on {}", args.listen_address);

    loop {
        let (mut socket_stream, addr) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buffer = [0; 1024];

            loop {
                let n = match socket_stream.read(&mut buffer).await {
                    Ok(n) if n == 0 => break,
                    Ok(n) => {
                        info!("Received {} BYTES\n", n);
                        n
                    }
                    Err(err) => {
                        error!("Error reading from peer {}: {}", addr, err);
                        break;
                    }
                };

                let message = String::from_utf8_lossy(&buffer[..n]);
                info!("Received {} bytes from {}: {}\n", n, addr, message);

                let response = format!("Received message. Hello, peer at {}\n", addr);
                match socket_stream.write_all(response.as_bytes()).await {
                    Ok(_) => {}
                    Err(err) => {
                        error!("Error writing to peer {}: {}", addr, err);
                        break;
                    }
                }
            }
        });
    }
}
