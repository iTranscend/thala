use std::net::SocketAddr;

use jsonrpsee::{IntoResponse, ResponsePayload};
use litep2p::PeerId;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ValidationError, validation::Validate};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: PeerId,
    pub peers: usize,
    pub connections: usize,
    pub listen_addr: SocketAddr,
    pub rpc_addr: Option<SocketAddr>,
}

impl IntoResponse for NodeInfo {
    type Output = NodeInfo;

    fn into_response(self) -> ResponsePayload<'static, Self::Output> {
        ResponsePayload::success(self)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Capabilities {
    pub cpu_cores: usize,
    pub memory: u64,
    pub nvidia_gpus: Vec<GraphicCard>,
    pub supported_models: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicCard {
    pub id: String,
    pub name: String,
    pub brand: nvml_wrapper::enum_wrappers::device::Brand,
    pub memory: u64,
    pub architecture: nvml_wrapper::enums::device::DeviceArchitecture,
    pub compute_mode: nvml_wrapper::enum_wrappers::device::ComputeMode,
}

#[derive(Clone, Deserialize, Debug, Serialize)]
pub struct TaskId {
    id: Uuid,
}

impl TaskId {
    pub fn new() -> Self {
        TaskId { id: Uuid::new_v4() }
    }

    pub fn id(&self) -> &Uuid {
        &self.id
    }
}

impl Validate for TaskId {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.id.is_nil() {
            Err(ValidationError::InvalidTaskId)
        } else {
            Ok(())
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum TaskType {
    Benchmark { model: String, dataset: String },
    Training,
    Inference,
}

#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    id: TaskId,
    _type: TaskType,
    status: TaskStatus,
}

impl Task {
    pub fn new(id: TaskId, _type: TaskType) -> Self {
        Task {
            id,
            _type,
            status: TaskStatus::Pending,
        }
    }

    pub fn id(&self) -> &TaskId {
        &self.id
    }
}
