use std::{
    hash::Hash,
    net::SocketAddr,
    time::{SystemTime, UNIX_EPOCH},
};

use jsonrpsee::{IntoResponse, ResponsePayload};
use litep2p::PeerId;
use serde::{Deserialize, Serialize};
use tracing::{Level, event};
use uuid::Uuid;

use crate::{
    error::ValidationError,
    validation::{MIN_TASK_EXPIRATION_TIME, Validate},
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NodeInfo {
    pub id: PeerId,
    pub peers: usize,
    pub connections: usize,
    pub listen_addr: SocketAddr,
    pub rpc_addr: Option<SocketAddr>,
    pub capabilities: Capabilities,
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
    pub supported_datasets: Vec<String>,
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

#[derive(Clone, Copy, Deserialize, Debug, Serialize, Hash, PartialEq, Eq)]
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

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Task {
    id: TaskId,
    kind: TaskType,
    status: TaskStatus,
    expires: u64,
}

impl Task {
    pub fn new(id: TaskId, kind: TaskType, expires: u64) -> Self {
        Task {
            id,
            kind,
            status: TaskStatus::Pending,
            expires,
        }
    }

    pub fn id(&self) -> TaskId {
        self.id
    }

    pub fn kind(&self) -> &TaskType {
        &self.kind
    }

    pub fn status(&self) -> &TaskStatus {
        &self.status
    }

    pub fn set_status(&mut self, status: TaskStatus) {
        self.status = status;
    }

    pub fn expires(&self) -> u64 {
        self.expires
    }
}

impl Validate for Task {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let remaining = self.expires.saturating_sub(now);
        if remaining < MIN_TASK_EXPIRATION_TIME {
            event!(
                Level::ERROR,
                "Task expiration time is too short: {}",
                self.expires
            );
            Err(ValidationError::InvalidExpires)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now_secs() -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn benchmark_task(expires: u64) -> Task {
        Task::new(
            TaskId::new(),
            TaskType::Benchmark {
                model: "m".to_string(),
                dataset: "d".to_string(),
            },
            expires,
        )
    }

    #[test]
    fn task_validate_accepts_valid_expiry() {
        let task = benchmark_task(now_secs() + 120);
        assert!(task.validate().is_ok());
    }

    #[test]
    fn task_validate_rejects_expiry_below_minimum() {
        // Expires within the minimum window -> rejected.
        let task = benchmark_task(now_secs() + 10);
        assert!(matches!(
            task.validate(),
            Err(ValidationError::InvalidExpires)
        ));
    }

    #[test]
    fn task_validate_handles_underflow_without_panic() {
        // expires far in the past -> saturating_sub yields 0, no panic, rejected.
        let task = benchmark_task(0);
        assert!(matches!(
            task.validate(),
            Err(ValidationError::InvalidExpires)
        ));
    }

    #[test]
    fn task_id_validate_rejects_nil() {
        let nil = TaskId { id: Uuid::nil() };
        assert!(matches!(
            nil.validate(),
            Err(ValidationError::InvalidTaskId)
        ));
    }

    #[test]
    fn task_id_validate_accepts_random() {
        assert!(TaskId::new().validate().is_ok());
    }
}
