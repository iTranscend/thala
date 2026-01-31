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
