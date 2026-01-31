use std::{collections::HashMap, net::SocketAddr};

use litep2p::PeerId;
use serde::{Deserialize, Serialize};
use shared::types::Capabilities;

use crate::types::{TaskId, TaskResultData, TaskType};

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionResp {
    pub peer_id: PeerId,
    pub listen_addr: SocketAddr,
    pub known_peers: HashMap<PeerId, (SocketAddr, Capabilities)>,
    pub message: Option<String>,
    pub capabilities: Capabilities,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ConnectionReq {
    pub peer_id: PeerId,
    pub listen_addr: SocketAddr,
    pub message: Option<String>,
    pub capabilities: Capabilities,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskAnnouncement {
    task_id: TaskId,
    task_type: TaskType,
    coordinator: PeerId,
    pub expires: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskClaim {
    task_id: TaskId,
    worker_id: PeerId,
    estimated_duration: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskResult {
    task_id: TaskId,
    result: TaskResultData,
    worker_id: PeerId,
    execution_time_ms: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum Message {
    ConnectToPeerReq(ConnectionReq),
    ConnectToPeerResp(ConnectionResp),
    Ping { timestamp_millis: u64 },
    Pong { timestamp_millis: u64 },

    TaskAnnouncement(TaskAnnouncement),
    TaskClaim(TaskClaim),
    TaskResult(TaskResult),
}
