use std::{collections::HashMap, net::SocketAddr};

use litep2p::PeerId;
use serde::{Deserialize, Serialize};
use shared::{
    error::ValidationError,
    types::{Capabilities, Task, TaskId},
    validation::Validate,
};

use crate::types::TaskResultData;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionResp {
    pub peer_id: PeerId,
    pub listen_addr: SocketAddr,
    pub known_peers: HashMap<PeerId, (SocketAddr, Capabilities)>,
    pub message: Option<String>,
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConnectionReq {
    pub peer_id: PeerId,
    pub listen_addr: SocketAddr,
    pub message: Option<String>,
    pub capabilities: Capabilities,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Coordinator {
    pub peer_id: PeerId,
    pub addr: SocketAddr,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskAnnouncement {
    pub task: Task,
    pub coordinator: Coordinator,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskClaim {
    pub task_id: TaskId,
    pub worker_id: PeerId,
    pub estimated_duration: u64,
}

impl Validate for TaskClaim {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // TODO: validate claimer's capabilities
        println!("Validate taskClaim");
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TaskResult {
    pub task_id: TaskId,
    pub result: TaskResultData,
    pub worker_id: PeerId,
    pub execution_time_ms: u64,
}

impl Validate for TaskResult {
    type Error = ValidationError;

    fn validate(&self) -> Result<(), Self::Error> {
        // Structural validation only — assignment verification requires Node context
        // and is performed in the TaskResult message handler.
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum Message {
    ConnectToPeerReq(ConnectionReq),
    ConnectToPeerResp(ConnectionResp),
    Ping { timestamp_millis: u64 },
    Pong { timestamp_millis: u64 },

    TaskAnnouncement(TaskAnnouncement),
    TaskClaim(TaskClaim),
    TaskResult(TaskResult),
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use litep2p::PeerId;
    use litep2p::crypto::{PublicKey, ed25519::Keypair};
    use shared::types::{Task, TaskId, TaskType};

    use super::*;
    use crate::types::TaskResultData;

    fn peer_id() -> PeerId {
        PeerId::from_public_key(&PublicKey::Ed25519(Keypair::generate().public()))
    }

    fn capabilities() -> Capabilities {
        Capabilities {
            cpu_cores: 4,
            memory: 16,
            nvidia_gpus: vec![],
            supported_models: vec!["m".to_string()],
            supported_datasets: vec!["d".to_string()],
        }
    }

    fn addr() -> SocketAddr {
        "127.0.0.1:2345".parse().unwrap()
    }

    fn sample_task() -> Task {
        Task::new(
            TaskId::new(),
            TaskType::Benchmark {
                model: "m".to_string(),
                dataset: "d".to_string(),
            },
            9_999_999_999,
        )
    }

    /// Assert a Message survives a postcard encode/decode/re-encode cycle.
    /// Message has no PartialEq, so we compare the encoded byte payloads.
    fn assert_round_trip(msg: Message) {
        let bytes = postcard::to_stdvec(&msg).expect("serialize");
        let decoded: Message = postcard::from_bytes(&bytes).expect("deserialize");
        let reencoded = postcard::to_stdvec(&decoded).expect("re-serialize");
        assert_eq!(bytes, reencoded);
    }

    #[test]
    fn round_trip_connect_req() {
        assert_round_trip(Message::ConnectToPeerReq(ConnectionReq {
            peer_id: peer_id(),
            listen_addr: addr(),
            message: Some("hi".to_string()),
            capabilities: capabilities(),
        }));
    }

    #[test]
    fn round_trip_connect_resp() {
        let mut known_peers = HashMap::new();
        known_peers.insert(peer_id(), (addr(), capabilities()));
        assert_round_trip(Message::ConnectToPeerResp(ConnectionResp {
            peer_id: peer_id(),
            listen_addr: addr(),
            known_peers,
            message: None,
            capabilities: capabilities(),
        }));
    }

    #[test]
    fn round_trip_ping_pong() {
        assert_round_trip(Message::Ping {
            timestamp_millis: 42,
        });
        assert_round_trip(Message::Pong {
            timestamp_millis: 42,
        });
    }

    #[test]
    fn round_trip_task_announcement() {
        assert_round_trip(Message::TaskAnnouncement(TaskAnnouncement {
            task: sample_task(),
            coordinator: Coordinator {
                peer_id: peer_id(),
                addr: addr(),
            },
        }));
    }

    #[test]
    fn round_trip_task_claim() {
        assert_round_trip(Message::TaskClaim(TaskClaim {
            task_id: TaskId::new(),
            worker_id: peer_id(),
            estimated_duration: 90,
        }));
    }

    #[test]
    fn round_trip_task_result() {
        assert_round_trip(Message::TaskResult(TaskResult {
            task_id: TaskId::new(),
            result: TaskResultData::Success {
                output: vec![1, 2, 3],
            },
            worker_id: peer_id(),
            execution_time_ms: 1234,
        }));
        assert_round_trip(Message::TaskResult(TaskResult {
            task_id: TaskId::new(),
            result: TaskResultData::Failure {
                error: "boom".to_string(),
                output: None,
            },
            worker_id: peer_id(),
            execution_time_ms: 5,
        }));
    }
}
