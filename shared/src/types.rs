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

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Apple,
}

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
    pub gpus: Vec<GraphicCard>,
    pub supported_models: Vec<String>,
    pub supported_datasets: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GraphicCard {
    pub id: String,
    pub name: String,
    pub vendor: GpuVendor,
    pub memory: u64,
    pub architecture: Option<String>,
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

    #[test]
    fn gpu_vendor_round_trip() {
        use postcard;
        for vendor in [GpuVendor::Nvidia, GpuVendor::Amd, GpuVendor::Apple] {
            let bytes = postcard::to_stdvec(&vendor).expect("serialize");
            let decoded: GpuVendor = postcard::from_bytes(&bytes).expect("deserialize");
            assert_eq!(vendor, decoded);
        }
    }

    #[test]
    fn graphic_card_round_trip() {
        use postcard;
        let card = GraphicCard {
            id: "gpu-0".to_string(),
            name: "Test GPU".to_string(),
            vendor: GpuVendor::Nvidia,
            memory: 8_000_000_000,
            architecture: Some("Ampere".to_string()),
        };
        let bytes = postcard::to_stdvec(&card).expect("serialize");
        let decoded: GraphicCard = postcard::from_bytes(&bytes).expect("deserialize");
        assert_eq!(decoded.id, card.id);
        assert_eq!(decoded.vendor, card.vendor);
        assert_eq!(decoded.memory, card.memory);
        assert_eq!(decoded.architecture, card.architecture);
    }
}
