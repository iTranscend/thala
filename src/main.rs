#![feature(never_type)]

use std::error::Error;

use clap::Parser;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

mod cli;

#[tokio::main]
async fn main() -> Result<!, Box<dyn Error>> {
    let args = cli::Args::parse();

    if let Some(bootstrap_node) = args.bootstrap_node {
        println!("Connecting to bootstrap node at {}", bootstrap_node);

        let mut stream = TcpStream::connect(bootstrap_node).await?;
        println!("Connected to bootstrap node!");

        let message = "Sup bootstrap node";
        stream.write_all(message.as_bytes()).await?;

        let mut buffer = [0; 1024];
        let n = stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        println!(
            "Received {} bytes from bootstrap node {} translated to : {}",
            n, bootstrap_node, response
        );
    }

    let listener = TcpListener::bind(args.listen_address).await?;
    println!("Node listening on {}", args.listen_address);

    loop {
        let (mut socket_stream, addr) = listener.accept().await?;

        tokio::spawn(async move {
            let mut buffer = [0; 1024];

            loop {
                let n = match socket_stream.read(&mut buffer).await {
                    Ok(n) if n == 0 => break,
                    Ok(n) => {
                        println!("Received {} BYTES\n", n);
                        n
                    }
                    Err(err) => {
                        eprintln!("Error reading from peer {}: {}", addr, err);
                        break;
                    }
                };

                let message = String::from_utf8_lossy(&buffer[..n]);
                println!("Received {} bytes from {}: {}\n", n, addr, message);

                let response = format!("Received message. Hello, peer at {}\n", addr);
                match socket_stream.write_all(response.as_bytes()).await {
                    Ok(_) => {}
                    Err(err) => {
                        eprintln!("Error writing to peer {}: {}", addr, err);
                        break;
                    }
                }
            }
        });
    }
}
