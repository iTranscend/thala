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
    pub gpu_memory: Option<u32>,
    pub supported_models: Vec<String>,
}
