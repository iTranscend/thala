# Thala

barebones p2p network for wasm contract execution


## Usage

Run bootnode
  ```sh
    cargo run -- -l 127.0.0.1:5674
  ```
  
Run node
  ```sh
  cargo run -- -b 127.0.0.1:5674 -l 127.0.0.1:2675
  ```

