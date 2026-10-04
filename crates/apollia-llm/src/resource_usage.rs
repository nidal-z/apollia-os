//! What the machine is using right now: memory, processor, video memory.
//!
//! One measurement, taken when asked and never kept running: the caller
//! decides how often it reads. The video memory is measured where a tool the
//! machine already has can say it (`nvidia-smi` on an NVIDIA card); on Apple
//! Silicon it is the system memory itself, and anywhere else it is stated as
//! not measured rather than guessed.

use serde::{Deserialize, Serialize};
use sysinfo::System;

const BYTES_PER_GB: f64 = 1_073_741_824.0;
#[cfg(any(not(target_os = "macos"), test))]
const MIB_PER_GB: f64 = 1024.0;

/// The memory and the processor the machine is using at the time of the call.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResourceUsage {
    /// System memory in use, in GB.
    pub ram_used_gb: f64,
    /// System memory installed, in GB.
    pub ram_total_gb: f64,
    /// Processor load over every core, from 0 to 100.
    pub cpu_percent: f32,
    /// Video memory in use.
    pub vram: VramUsage,
}

/// The video memory in use, or why no figure is given.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum VramUsage {
    /// Memory of the graphics cards, summed over the cards the driver lists.
    Dedicated {
        /// Video memory in use, in GB.
        used_gb: f64,
        /// Video memory installed, in GB.
        total_gb: f64,
    },
    /// The graphics processor shares the system memory: the RAM figures are
    /// the video memory figures.
    Unified,
    /// No tool on this machine states the video memory in use.
    Unmeasured,
}

/// A measurement that did not reach its end.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum ResourceUsageError {
    /// The thread that asks the driver for the video memory did not finish.
    #[error("the video memory probe did not finish")]
    VramProbe(#[source] tokio::task::JoinError),
}

/// Measure the machine once.
///
/// Takes [`sysinfo::MINIMUM_CPU_UPDATE_INTERVAL`] to answer: a processor load
/// is a difference between two readings, so one reading alone says nothing.
/// The wait between the two is the timer's, so no thread is held for it; only
/// the video memory probe, which may start a process, runs on a blocking
/// thread.
///
/// # Errors
///
/// [`ResourceUsageError::VramProbe`] when the video memory probe is cancelled
/// or panics.
pub async fn measure() -> Result<ResourceUsage, ResourceUsageError> {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_usage();
    tokio::time::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL).await;
    sys.refresh_cpu_usage();

    let vram = tokio::task::spawn_blocking(measure_vram)
        .await
        .map_err(ResourceUsageError::VramProbe)?;

    Ok(ResourceUsage {
        ram_used_gb: sys.used_memory() as f64 / BYTES_PER_GB,
        ram_total_gb: sys.total_memory() as f64 / BYTES_PER_GB,
        cpu_percent: sys.global_cpu_usage().clamp(0.0, 100.0),
        vram,
    })
}

#[cfg(target_os = "macos")]
fn measure_vram() -> VramUsage {
    if cfg!(target_arch = "aarch64") {
        VramUsage::Unified
    } else {
        VramUsage::Unmeasured
    }
}

#[cfg(not(target_os = "macos"))]
fn measure_vram() -> VramUsage {
    let mut cmd = std::process::Command::new("nvidia-smi");
    cmd.args([
        "--query-gpu=memory.used,memory.total",
        "--format=csv,noheader,nounits",
    ]);
    apollia_core::subprocess_window::hide_console(&mut cmd);
    match cmd.output() {
        Ok(output) if output.status.success() => {
            parse_nvidia_memory(&String::from_utf8_lossy(&output.stdout))
                .unwrap_or(VramUsage::Unmeasured)
        }
        _ => VramUsage::Unmeasured,
    }
}

/// Read `nvidia-smi --query-gpu=memory.used,memory.total` in MiB, one card per
/// line, and sum the cards. `None` when no line carries two numbers.
#[cfg(any(not(target_os = "macos"), test))]
fn parse_nvidia_memory(listing: &str) -> Option<VramUsage> {
    let mut cards = listing.lines().filter_map(|line| {
        let (used, total) = line.split_once(',')?;
        let used: f64 = used.trim().parse().ok()?;
        let total: f64 = total.trim().parse().ok()?;
        Some((used, total))
    });
    let first = cards.next()?;
    let (used, total) = cards.fold(first, |sum, card| (sum.0 + card.0, sum.1 + card.1));
    Some(VramUsage::Dedicated {
        used_gb: used / MIB_PER_GB,
        total_gb: total / MIB_PER_GB,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_a_measurement_states_memory_within_what_is_installed_and_a_bounded_load() {
        // GIVEN this machine
        // WHEN it is measured once
        let usage = measure().await.expect("measurement");

        // THEN the memory in use is positive and within what is installed, and
        //      the processor load is a percentage
        assert!(usage.ram_total_gb > 0.0);
        assert!(usage.ram_used_gb > 0.0);
        assert!(usage.ram_used_gb <= usage.ram_total_gb);
        assert!((0.0..=100.0).contains(&usage.cpu_percent));
    }

    #[tokio::test]
    async fn test_apple_silicon_states_unified_memory_and_never_a_figure() {
        // GIVEN the machine the test runs on
        // WHEN its video memory is measured
        let vram = measure().await.expect("measurement").vram;

        // THEN an Apple chip says the memory is unified; any other machine
        //      says a figure or that none is measured, never unified
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            assert_eq!(vram, VramUsage::Unified);
        } else {
            assert_ne!(vram, VramUsage::Unified);
        }
    }

    #[test]
    fn test_one_card_reads_as_its_used_and_installed_memory() {
        // GIVEN the listing of one card, in MiB
        let listing = "2048, 24576\n";

        // WHEN it is read
        let vram = parse_nvidia_memory(listing);

        // THEN the figures are that card's, in GB
        assert_eq!(
            vram,
            Some(VramUsage::Dedicated {
                used_gb: 2.0,
                total_gb: 24.0
            })
        );
    }

    #[test]
    fn test_two_cards_are_summed() {
        // GIVEN the listing of two cards
        let listing = "1024, 8192\n3072, 8192\n";

        // WHEN it is read
        let vram = parse_nvidia_memory(listing);

        // THEN used and installed memory are the sums over both
        assert_eq!(
            vram,
            Some(VramUsage::Dedicated {
                used_gb: 4.0,
                total_gb: 16.0
            })
        );
    }

    #[test]
    fn test_a_listing_without_figures_gives_nothing() {
        // GIVEN what the tool prints when a card does not report its memory
        let listing = "[N/A], [N/A]\n";

        // WHEN it is read
        let vram = parse_nvidia_memory(listing);

        // THEN no figure is made up
        assert_eq!(vram, None);
    }

    #[test]
    fn test_the_video_memory_is_tagged_by_kind_on_the_wire() {
        // GIVEN the three answers a measurement can give
        let dedicated = VramUsage::Dedicated {
            used_gb: 1.5,
            total_gb: 8.0,
        };

        // WHEN they are serialised
        let wire = |v: &VramUsage| serde_json::to_value(v).unwrap();

        // THEN each carries its kind, and only the dedicated one carries figures
        assert_eq!(
            wire(&dedicated),
            serde_json::json!({ "kind": "dedicated", "used_gb": 1.5, "total_gb": 8.0 })
        );
        assert_eq!(
            wire(&VramUsage::Unified),
            serde_json::json!({ "kind": "unified" })
        );
        assert_eq!(
            wire(&VramUsage::Unmeasured),
            serde_json::json!({ "kind": "unmeasured" })
        );
    }
}
