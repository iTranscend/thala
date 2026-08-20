//! GPU detection — aggregates results from per-vendor backends.

mod nvidia;

#[cfg(target_os = "macos")]
mod metal;

#[cfg(target_os = "linux")]
mod amd;

use shared::types::GraphicCard;
use tracing::{Level, event};

/// Detect all GPUs across every available vendor backend.
/// Each backend handles its own errors internally and returns an empty vec on
/// failure, so this function never errors.
pub fn detect_all() -> Vec<GraphicCard> {
    let mut gpus = Vec::new();

    gpus.extend(nvidia::detect());

    #[cfg(target_os = "macos")]
    gpus.extend(metal::detect());

    #[cfg(target_os = "linux")]
    gpus.extend(amd::detect());

    event!(Level::TRACE, "Detected {} GPU(s) total", gpus.len());
    gpus
}
