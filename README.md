# Thala

A barebones peer-to-peer network implementation in Rust.

## Features

- **Peer Discovery**: Automatic peer discovery through bootstrap nodes
- **Connection Management**: Maintains persistent connections with automatic reconnection
- **Exponential Backoff**: Smart retry logic with configurable backoff parameters
- **Persistent Identity**: Node keys are saved to disk for identity persistence across restarts
- **JSON-RPC Server**: Optional HTTP server for querying node state
- **Keep-Alive**: Heartbeat mechanism to keep connections alive

## Quick Start

Ensure [Rust](https://rust-lang.org/tools/install/) is installed

### 1. Run a Bootstrap Node

Start a bootstrap node that other peers can connect to:

```sh
cargo run -- -l 127.0.0.1:5674
```

### 2. Connect Additional Nodes

Start additional nodes and connect them to the bootstrap node:

```sh
cargo run -- -b 127.0.0.1:5674 -l 127.0.0.1:2675
```

Each node will:
- Connect to the bootstrap node
- Discover other peers in the network
- Maintain persistent connections with automatic reconnection

### 3. Run with RPC Server

Enable the RPC server to query node information:

```sh
cargo run -- -l 127.0.0.1:2345 --rpc-addr 127.0.0.1:3330
```

## Configuration

### Command-Line Options

```
Usage: thala [OPTIONS]

Options:
  -l, --listen-address <LISTEN_ADDRESS>
          Address and port to listen on for peer connections
          [default: 127.0.0.1:2345]

  -b, --bootstrap-node <BOOTSTRAP_NODE>
          Bootstrap node address to connect to on startup

  -p, --peer-reconnection-interval <PEER_RECONNECTION_INTERVAL>
          Time interval in seconds between reconnection attempts
          [default: 30]

  -x, --backoff-multiplier <BACKOFF_MULTIPLIER>
          Multiplier for exponential backoff on failed reconnections
          [default: 2.0]

  -m, --max-backoff-interval <MAX_BACKOFF_INTERVAL>
          Maximum backoff interval in seconds
          [default: 2000]

  -r, --reconnection-retries-cap <RECONNECTION_RETRIES_CAP>
          Maximum number of reconnection attempts before giving up
          [default: 13]

  --rpc-addr <RPC_ADDR>
          RPC server listening address (optional)

  -d, --data-dir <DATA_DIR>
          Data directory for storing node identity and state
          [default: ~/.thala]

  -h, --help
          Print help information
```

### Example: Custom Configuration

```sh
cargo run -- \
  -b 127.0.0.1:2345 \
  -l 127.0.0.1:2346 \
  -p 30 \
  -m 300 \
  -x 2.0 \
  -r 10 \
  --rpc-addr 127.0.0.1:3331 \
  -d ./my-node-data
```

## RPC API

When `--rpc-addr` is specified, a JSON-RPC server is started at the given address.

### Available Methods

| Method | Description | Returns |
|--------|-------------|---------|
| `info` | Get node information | Node ID, listen address, peer counts |
| `peers` | Get all known peers | List of peer IDs and addresses |
| `connections` | Get active connections | List of currently connected peers |

### Example

**List active connections:**
```sh
curl -X POST http://127.0.0.1:3330 \
  -H "Content-Type: application/json" \
  -d '{
    "jsonrpc": "2.0",
    "method": "connections",
    "params": [],
    "id": 3
  }'
```

### Peer Reconnection

The node implements exponential backoff for reconnection attempts:

1. Initial reconnection interval: `peer-reconnection-interval` seconds
2. After each failed attempt, the interval is multiplied by `backoff-multiplier`
3. The interval is capped at `max-backoff-interval` seconds
4. After `reconnection-retries-cap` failures, the peer is considered dead

## Environment Variables

Set `RUST_LOG` to control logging verbosity:

```sh
# Show all logs
RUST_LOG=trace cargo run

# Show only warnings and errors
RUST_LOG=warn cargo run

# Show info from thala, debug from other modules
RUST_LOG=thala=info,debug cargo run
```
