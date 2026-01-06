use bincode::{Decode, Encode};
use uuid::Uuid;

use crate::{error::ValidationError, validation::Validate};

#[derive(Encode, Decode, Debug)]
pub struct TaskId {
    #[bincode(with_serde)]
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

#[derive(Encode, Decode, Debug)]
pub enum TaskType {
    Benchmark { model: String, dataset: String },
    Training,
    Inference,
}

#[derive(Encode, Decode, Debug)]
pub enum TaskResultData {
    Success {
        output: Vec<u8>,
    },
    Failure {
        error: String,
        output: Option<Vec<u8>>,
    },
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct Capabilities {
    pub cpu_cores: usize,
    pub memory: u64,
    pub nvidia_gpus: Vec<GraphicCard>,
    pub supported_models: Vec<String>,
}

#[derive(Encode, Decode, Debug, Clone)]
pub struct GraphicCard {
    #[bincode(with_serde)]
    pub id: String,
    pub name: String,
    #[bincode(with_serde)]
    pub brand: nvml_wrapper::enum_wrappers::device::Brand,
    pub memory: u64,
    #[bincode(with_serde)]
    pub architecture: nvml_wrapper::enums::device::DeviceArchitecture,
    #[bincode(with_serde)]
    pub compute_mode: nvml_wrapper::enum_wrappers::device::ComputeMode,
}
