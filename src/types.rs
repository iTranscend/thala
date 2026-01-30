use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{error::ValidationError, validation::Validate};

#[derive(Deserialize, Debug, Serialize)]
pub struct TaskId {
    id: Uuid,
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

#[derive(Serialize, Deserialize, Debug)]
pub enum TaskType {
    Benchmark { model: String, dataset: String },
    Training,
    Inference,
}

#[derive(Deserialize, Debug, Serialize)]
pub enum TaskResultData {
    Success {
        output: Vec<u8>,
    },
    Failure {
        error: String,
        output: Option<Vec<u8>>,
    },
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
