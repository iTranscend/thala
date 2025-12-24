use std::{collections::HashMap, net::SocketAddr};

use bincode::{Decode, Encode};
use litep2p::PeerId;

#[derive(Encode, Decode, Debug)]
pub struct ConnectionResp {
    #[bincode(with_serde)]
    pub peer_id: PeerId,
    pub listen_addr: SocketAddr,
    #[bincode(with_serde)]
    pub known_peers: HashMap<PeerId, SocketAddr>,
    pub message: Option<String>,
}

#[derive(Encode, Decode, Debug)]
pub struct ConnectionReq {
    #[bincode(with_serde)]
    pub peer_id: PeerId,
    pub listen_addr: SocketAddr,
    pub message: Option<String>,
}

#[derive(Encode, Decode, Debug)]
pub enum Message {
    ConnectToPeerReq(ConnectionReq),
    ConnectToPeerResp(ConnectionResp),
    Ping { timestamp_millis: u64 },
    Pong { timestamp_millis: u64 },
}
