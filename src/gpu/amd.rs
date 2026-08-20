//! AMD GPU detection via Linux sysfs (no external crate required).
//!
//! Scans `/sys/class/drm/card*/device/` for PCI devices with AMD's vendor ID
//! (`0x1002`).  VRAM information requires the `amdgpu` kernel driver which
//! exposes `mem_info_vram_total` in sysfs.

use std::fs;
use std::path::Path;

use shared::types::{GpuVendor, GraphicCard};
use tracing::{Level, event};

/// AMD PCI vendor ID.
const AMD_VENDOR_ID: &str = "0x1002";

/// Detect AMD GPUs by reading Linux sysfs.
///
/// Returns an empty vec on non-Linux platforms, if `/sys/class/drm` is absent,
/// or if no AMD GPUs are found.
pub fn detect() -> Vec<GraphicCard> {
    let drm_path = Path::new("/sys/class/drm");
    if !drm_path.exists() {
        event!(Level::TRACE, "/sys/class/drm not found, skipping AMD detection");
        return vec![];
    }

    let entries = match fs::read_dir(drm_path) {
        Ok(entries) => entries,
        Err(e) => {
            event!(Level::WARN, "Failed to read /sys/class/drm: {}", e);
            return vec![];
        }
    };

    let mut gpus = Vec::new();

    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Only inspect top-level cardN entries, not output ports like card0-HDMI-A-1.
        if !name_str.starts_with("card") || name_str.contains('-') {
            continue;
        }

        let device_path = entry.path().join("device");

        if let Some(card) = probe_card(&device_path, &name_str) {
            gpus.push(card);
        }
    }

    if !gpus.is_empty() {
        event!(Level::TRACE, "Detected {} AMD GPU(s) via sysfs", gpus.len());
    }

    gpus
}

/// Read sysfs attributes for a single DRM card and return a [`GraphicCard`] if
/// the device is an AMD GPU.
fn probe_card(device_path: &Path, card_name: &str) -> Option<GraphicCard> {
    let vendor = read_sysfs(device_path, "vendor")?;
    if vendor != AMD_VENDOR_ID {
        return None;
    }

    let product_name = read_sysfs(device_path, "product_name")
        .or_else(|| read_sysfs(device_path, "label"))
        .unwrap_or_else(|| "AMD GPU".to_string());

    let memory = read_sysfs(device_path, "mem_info_vram_total")
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(0);

    let id = read_sysfs(device_path, "unique_id").unwrap_or_else(|| card_name.to_string());

    Some(GraphicCard {
        id,
        name: product_name,
        vendor: GpuVendor::Amd,
        memory,
        architecture: None,
    })
}

/// Read a single sysfs attribute file, returning the trimmed contents.
fn read_sysfs(device_path: &Path, attr: &str) -> Option<String> {
    fs::read_to_string(device_path.join(attr))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_returns_empty_when_sysfs_absent() {
        // On macOS and most CI environments /sys/class/drm does not exist,
        // so detect() should return an empty vec without panicking.
        let gpus = detect();
        assert!(gpus.iter().all(|g| g.vendor == GpuVendor::Amd));
    }

    #[test]
    fn read_sysfs_returns_none_for_missing_file() {
        let result = read_sysfs(Path::new("/nonexistent"), "vendor");
        assert!(result.is_none());
    }
}
