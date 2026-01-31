use std::{collections::HashMap, net::SocketAddr};

use litep2p::PeerId;
use serde::{Deserialize, Serialize};
use shared::{types::{Capabilities,Task, TaskId}, validation::{Validate, MIN_TASK_EXPIRATION_TIME}, error::ValidationError};

use crate::types::{ TaskResultData};

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
    pub task: Task,
    pub coordinator: PeerId,
    pub expires: u64,
}

impl Validate for TaskAnnouncement {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.expires < MIN_TASK_EXPIRATION_TIME {
            Err(ValidationError::InvalidExpires)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskClaim {
    task_id: TaskId,
    worker_id: PeerId,
    estimated_duration: u64,
}

impl Validate for TaskClaim {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // TODO: validate claimer's capabilities
        todo!()
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TaskResult {
    task_id: TaskId,
    result: TaskResultData,
    worker_id: PeerId,
    execution_time_ms: u64,
}

impl Validate for TaskResult {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // TODO: verify that result is from peer that claimed task.
        todo!()
    }
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
