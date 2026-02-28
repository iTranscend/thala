use serde::{Deserialize, Serialize};

#[derive(Clone, Deserialize, Debug, Serialize)]
pub enum TaskResultData {
    Success {
        output: Vec<u8>,
    },
    Failure {
        error: String,
        output: Option<Vec<u8>>,
    },
}
