//! NVIDIA GPU detection via the NVML library.

use nvml_wrapper::Nvml;
use shared::types::{GpuVendor, GraphicCard};
use tracing::{Level, event};

/// Detect NVIDIA GPUs using the NVML (NVIDIA Management Library).
///
/// Returns an empty vec if the NVML library is unavailable or no NVIDIA GPUs
/// are present. Individual device query failures are logged and skipped rather
/// than aborting the entire detection.
pub fn detect() -> Vec<GraphicCard> {
    let nvml = match Nvml::init() {
        Ok(nvml) => nvml,
        Err(e) => {
            event!(
                Level::TRACE,
                "NVML unavailable, skipping NVIDIA detection: {}",
                e
            );
            return vec![];
        }
    };

    let device_count = match nvml.device_count() {
        Ok(count) => count,
        Err(e) => {
            event!(Level::WARN, "Failed to query NVML device count: {}", e);
            return vec![];
        }
    };

    let mut gpus = Vec::new();

    for i in 0..device_count {
        match probe_device(&nvml, i) {
            Ok(card) => gpus.push(card),
            Err(e) => {
                event!(Level::WARN, "Failed to probe NVIDIA GPU {}: {}", i, e);
            }
        }
    }

    if !gpus.is_empty() {
        event!(Level::TRACE, "Detected {} NVIDIA GPU(s)", gpus.len());
    }

    gpus
}

/// Query a single NVML device by index and build a [`GraphicCard`].
fn probe_device(nvml: &Nvml, index: u32) -> Result<GraphicCard, nvml_wrapper::error::NvmlError> {
    let device = nvml.device_by_index(index)?;

    let architecture = match device.architecture() {
        Ok(arch) => Some(format!("{:?}", arch)),
        Err(_) => None,
    };

    Ok(GraphicCard {
        id: device.uuid()?,
        name: device.name()?,
        vendor: GpuVendor::Nvidia,
        memory: device.memory_info()?.total,
        architecture,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_empty_when_nvml_unavailable() {
        // On machines without NVIDIA GPUs (CI, macOS), detect() should return
        // an empty vec without panicking.
        let gpus = detect();
        // We can't assert the exact count (it depends on hardware), but we can
        // verify it doesn't panic and returns a valid vec.
        assert!(gpus.iter().all(|g| g.vendor == GpuVendor::Nvidia));
    }
}
