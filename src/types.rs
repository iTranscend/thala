use serde::{Deserialize, Serialize};

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
