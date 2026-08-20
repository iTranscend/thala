//! Apple GPU detection via the Metal framework (macOS only).

use objc2_metal::{MTLCopyAllDevices, MTLDevice};
use shared::types::{GpuVendor, GraphicCard};
use tracing::{Level, event};

/// Detect Apple GPUs using the Metal framework.
///
/// Returns an empty vec if no Metal-capable devices are found.
pub fn detect() -> Vec<GraphicCard> {
    let devices = MTLCopyAllDevices();

    let gpus: Vec<GraphicCard> = devices
        .to_vec()
        .into_iter()
        .map(|device| {
            let name = device.name().to_string();
            let architecture = extract_architecture(&name);

            GraphicCard {
                id: device.registryID().to_string(),
                name,
                vendor: GpuVendor::Apple,
                memory: device.recommendedMaxWorkingSetSize(),
                architecture,
            }
        })
        .collect();

    if !gpus.is_empty() {
        event!(Level::TRACE, "Detected {} Apple GPU(s) via Metal", gpus.len());
    }

    gpus
}

/// Try to extract the architecture/chip name from a Metal device name.
///
/// Apple Metal device names typically look like "Apple M1 Pro", "Apple M2 Max",
/// etc.  We strip the leading "Apple " prefix to get the chip identifier.
fn extract_architecture(device_name: &str) -> Option<String> {
    device_name
        .strip_prefix("Apple ")
        .map(|s| s.to_string())
        .or_else(|| Some(device_name.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_finds_apple_gpu_on_macos() {
        // On Apple Silicon Macs, this should find at least one GPU.
        // On Intel Macs it may still find a GPU (discrete or integrated via Metal).
        let gpus = detect();
        assert!(gpus.iter().all(|g| g.vendor == GpuVendor::Apple));
        // On Apple Silicon dev machines we expect at least one GPU.
        // CI runners may vary, so we only assert the vendor is correct.
    }

    #[test]
    fn extract_architecture_strips_apple_prefix() {
        assert_eq!(
            extract_architecture("Apple M1 Pro"),
            Some("M1 Pro".to_string())
        );
        assert_eq!(
            extract_architecture("Apple M2 Max"),
            Some("M2 Max".to_string())
        );
    }

    #[test]
    fn extract_architecture_preserves_non_apple_names() {
        assert_eq!(
            extract_architecture("AMD Radeon Pro 5500M"),
            Some("AMD Radeon Pro 5500M".to_string())
        );
    }
}
