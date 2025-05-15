use std::{collections::HashSet, net::SocketAddr};

use bincode::{Decode, Encode};

#[derive(Encode, Decode, Debug)]
pub struct ConnectionResp {
    pub listen_addr: SocketAddr,
    pub known_peers: HashSet<SocketAddr>,
    pub message: Option<String>,
}

#[derive(Encode, Decode, Debug)]
pub struct ConnectionReq {
    pub listen_addr: SocketAddr,
    pub message: Option<String>,
}

#[derive(Encode, Decode, Debug)]
pub enum Message {
    ConnectToPeerReq(ConnectionReq),
    ConnectToPeerResp(ConnectionResp),
}
