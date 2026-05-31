use std::{collections::HashMap, net::SocketAddr};

use litep2p::PeerId;
use serde::{Deserialize, Serialize};
use shared::{
    error::ValidationError,
    types::{Capabilities, Task, TaskId},
    validation::Validate,
};

use crate::types::TaskResultData;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionResp {
    pub peer_id: PeerId,
    pub listen_addr: SocketAddr,
    pub known_peers: HashMap<PeerId, (SocketAddr, Capabilities)>,
    pub message: Option<String>,
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionReq {
    pub peer_id: PeerId,
    pub listen_addr: SocketAddr,
    pub message: Option<String>,
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Coordinator {
    pub peer_id: PeerId,
    pub addr: SocketAddr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskAnnouncement {
    pub task: Task,
    pub coordinator: Coordinator,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskClaim {
    pub task_id: TaskId,
    pub worker_id: PeerId,
    pub estimated_duration: u64,
}

impl Validate for TaskClaim {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // TODO: validate claimer's capabilities
        println!("Validate taskClaim");
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: TaskId,
    pub result: TaskResultData,
    pub worker_id: PeerId,
    pub execution_time_ms: u64,
}

impl Validate for TaskResult {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Structural validation only — assignment verification requires Node context
        // and is performed in the TaskResult message handler.
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Message {
    ConnectToPeerReq(ConnectionReq),
    ConnectToPeerResp(ConnectionResp),
    Ping { timestamp_millis: u64 },
    Pong { timestamp_millis: u64 },

    TaskAnnouncement(TaskAnnouncement),
    TaskClaim(TaskClaim),
    TaskResult(TaskResult),
}
