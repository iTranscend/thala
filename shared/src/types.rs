use std::net::SocketAddr;

use jsonrpsee::{IntoResponse, ResponsePayload};
use litep2p::PeerId;
use serde::{Deserialize, Serialize};

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
