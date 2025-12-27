use std::{collections::HashMap, net::SocketAddr};

use bincode::{Decode, Encode};
use litep2p::PeerId;

use crate::types::{Capabilities, TaskId, TaskResultData, TaskType};

#[derive(Encode, Decode, Debug)]
pub struct ConnectionResp {
    #[bincode(with_serde)]
    pub peer_id: PeerId,
    pub listen_addr: SocketAddr,
    #[bincode(with_serde)]
    pub known_peers: HashMap<PeerId, SocketAddr>,
    pub message: Option<String>,
    pub capabilities: Capabilities,
}

#[derive(Encode, Decode, Debug)]
pub struct ConnectionReq {
    #[bincode(with_serde)]
    pub peer_id: PeerId,
    pub listen_addr: SocketAddr,
    pub message: Option<String>,
    pub capabilities: Capabilities,
}

#[derive(Encode, Decode, Debug)]
pub struct TaskAnnouncement {
    task_id: TaskId,
    task_type: TaskType,
    #[bincode(with_serde)]
    coordinator: PeerId,
    pub expires: u64,
}

#[derive(Encode, Decode, Debug)]
pub struct TaskClaim {
    task_id: TaskId,
    #[bincode(with_serde)]
    worker_id: PeerId,
    estimated_duration: u64,
}

#[derive(Encode, Decode, Debug)]
pub struct TaskResult {
    task_id: TaskId,
    result: TaskResultData,
    #[bincode(with_serde)]
    worker_id: PeerId,
    execution_time_ms: u64,
}

#[derive(Encode, Decode, Debug)]
pub enum Message {
    ConnectToPeerReq(ConnectionReq),
    ConnectToPeerResp(ConnectionResp),
    Ping { timestamp_millis: u64 },
    Pong { timestamp_millis: u64 },

    TaskAnnouncement(TaskAnnouncement),
    TaskClaim(TaskClaim),
    TaskResult(TaskResult),
}
