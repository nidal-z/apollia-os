//! The graphics devices of the machine, one by one, and the ones held back.
//!
//! The list is the engine's own (`llama-server --list-devices`): a card the
//! operating system reports is not a card the engine can load onto, and the
//! identifier the engine prints is the only one its `--device` option reads.
//!
//! A device held back receives no layer of a model, and its memory leaves the
//! profile every estimate reads: [`select`] folds the choice into the
//! accelerator, so the badge, the guardrail and the offload plan count the
//! same memory the engine is given.

use std::sync::{OnceLock, RwLock};

use serde::{Deserialize, Serialize};

use super::AcceleratorProfile;

/// One graphics device of the machine.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GpuDevice {
    /// The identifier the engine lists it under (`CUDA0`, `Vulkan1`, `MTL0`),
    /// which its `--device` option reads. `None` when the engine could not be
    /// asked and the device is described from the hardware profile: such a
    /// device cannot be held back.
    pub id: Option<String>,
    /// Name of the device (`NVIDIA GeForce RTX 4090`).
    pub name: String,
    /// The interface the engine drives it through (`CUDA`, `Vulkan`, `Metal`).
    pub api: String,
    /// Memory of the device, in GB.
    pub vram_gb: f64,
    /// Whether the engine may load onto it.
    pub enabled: bool,
}

/// The interface an engine identifier names: the identifier without its index.
fn api_of(id: &str) -> String {
    let prefix = id.trim_end_matches(|c: char| c.is_ascii_digit());
    match prefix {
        "MTL" => "Metal".to_owned(),
        other => other.to_owned(),
    }
}

/// One line of `llama-server --list-devices`, as a device.
///
/// The shape is `  <backend><index>: <name> (<total> MiB, <free> MiB free)`.
/// The heading line carries no parenthesis and is refused by the same rule
/// that reads the others. A line whose memory reads zero is refused too: it
/// is how the engine lists its processor backend (`BLAS: Accelerate`), and a
/// card sized at zero would size every recommendation to nothing.
pub(super) fn parse_engine_device(line: &str) -> Option<GpuDevice> {
    let (id, rest) = line.trim().split_once(':')?;
    let id = id.trim();
    if id.is_empty() || id.contains(char::is_whitespace) {
        return None;
    }
    let rest = rest.trim();
    let open = rest.rfind('(')?;
    let name = rest[..open].trim();
    if name.is_empty() {
        return None;
    }
    let mib: String = rest[open + 1..]
        .trim_start()
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    let mib: f64 = mib.parse().ok()?;
    if mib <= 0.0 {
        return None;
    }
    Some(GpuDevice {
        id: Some(id.to_owned()),
        name: name.to_owned(),
        api: api_of(id),
        vram_gb: mib / 1024.0,
        enabled: true,
    })
}

/// Every device of a whole `--list-devices` output.
pub(super) fn parse_engine_devices(listing: &str) -> Vec<GpuDevice> {
    listing.lines().filter_map(parse_engine_device).collect()
}

/// Spawned once per process: several API routes read the profile on every
/// page load, and a subprocess each time would be worse than a stale list.
static ENGINE_DEVICES: OnceLock<Vec<GpuDevice>> = OnceLock::new();

/// The devices the bundled engine offers, empty when it cannot be asked.
pub(super) fn engine_devices() -> &'static [GpuDevice] {
    ENGINE_DEVICES.get_or_init(|| probe_engine_devices().unwrap_or_default())
}

fn probe_engine_devices() -> Option<Vec<GpuDevice>> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?;
    let ext = if cfg!(windows) { ".exe" } else { "" };
    let file = format!("llama-server{ext}");
    let mut candidates = vec![dir.join(&file)];
    candidates.extend(
        apollia_core::paths::bundled_resource_dirs(dir, "runners")
            .into_iter()
            .map(|d| d.join(&file)),
    );
    let binary = candidates.into_iter().find(|c| c.exists())?;

    let mut cmd = std::process::Command::new(binary);
    cmd.arg("--list-devices");
    apollia_core::subprocess_window::hide_console(&mut cmd);
    let out = cmd.output().ok()?;
    if !out.status.success() {
        return None;
    }
    Some(parse_engine_devices(&String::from_utf8_lossy(&out.stdout)))
}

/// The identifiers held back, process-wide.
///
/// The profile is read from some thirty places that know nothing of the
/// settings file, and all of them must count the same memory. The owner of
/// the setting (the runtime, at every load and every read of the hardware)
/// lays its value here; [`super::detect`] reads it. It is a cache of one
/// setting, like the probe beside it, and no actor shares state through it.
static DISABLED: RwLock<Vec<String>> = RwLock::new(Vec::new());

/// Hold back the devices named by `ids`, replacing the previous choice. An
/// empty slice gives the engine every device again.
pub fn set_disabled_devices(ids: &[String]) {
    if let Ok(mut held) = DISABLED.write() {
        if held.as_slice() != ids {
            *held = ids.to_vec();
        }
    }
}

/// The identifiers held back now.
#[must_use]
pub fn disabled_devices() -> Vec<String> {
    DISABLED.read().map(|held| held.clone()).unwrap_or_default()
}

/// The device a profile describes when the engine listed none: named, but
/// without an identifier, so it is shown and cannot be held back.
fn described(accelerator: &AcceleratorProfile) -> Option<GpuDevice> {
    let (name, api, vram_gb) = match accelerator {
        AcceleratorProfile::None => return None,
        AcceleratorProfile::AppleSilicon { chip, vram_gb, .. } => {
            (format!("Apple {chip}"), "Metal", *vram_gb)
        }
        AcceleratorProfile::Cuda {
            device_name,
            vram_gb,
            ..
        } => (device_name.clone(), "CUDA", *vram_gb),
        AcceleratorProfile::Generic {
            device_name,
            vram_gb,
        } => (device_name.clone(), "Vulkan", *vram_gb),
    };
    Some(GpuDevice {
        id: None,
        name,
        api: api.to_owned(),
        vram_gb,
        enabled: true,
    })
}

/// The accelerator the estimates read and the devices the interface lists,
/// from what was `detected`, what the engine `listed`, and the identifiers
/// `disabled`.
///
/// - **Apple Silicon** has one device, which is never held back: the list is
///   ignored, and the MLX engine has no device to choose anyway.
/// - **No listing** (the engine is absent or refused to run): the detected
///   accelerator is shown as one device without an identifier, and nothing
///   can be held back.
/// - **Every device held back**: no accelerator at all, the machine is sized
///   as a processor-only one.
/// - **Otherwise** the accelerator carries the summed memory of the devices
///   left on. A single device left alone keeps the detected figure, which may
///   come from a finer probe than the listing.
pub(super) fn select(
    detected: &AcceleratorProfile,
    listed: &[GpuDevice],
    disabled: &[String],
) -> (AcceleratorProfile, Vec<GpuDevice>) {
    if matches!(detected, AcceleratorProfile::AppleSilicon { .. }) {
        let devices = if listed.is_empty() {
            described(detected).into_iter().collect()
        } else {
            listed.to_vec()
        };
        return (detected.clone(), devices);
    }
    if listed.is_empty() {
        return (detected.clone(), described(detected).into_iter().collect());
    }

    let devices: Vec<GpuDevice> = listed
        .iter()
        .map(|device| GpuDevice {
            enabled: !device
                .id
                .as_ref()
                .is_some_and(|id| disabled.iter().any(|held| held == id)),
            ..device.clone()
        })
        .collect();
    let on: Vec<&GpuDevice> = devices.iter().filter(|d| d.enabled).collect();
    let Some(first) = on.first() else {
        return (AcceleratorProfile::None, devices);
    };
    if devices.len() == 1 && !matches!(detected, AcceleratorProfile::None) {
        return (detected.clone(), devices);
    }
    let vram_gb: f64 = on.iter().map(|d| d.vram_gb).sum();
    let device_name = first.name.clone();
    let accelerator = match detected {
        AcceleratorProfile::Cuda {
            compute_capability, ..
        } => AcceleratorProfile::Cuda {
            device_name,
            vram_gb,
            compute_capability: *compute_capability,
        },
        _ => AcceleratorProfile::Generic {
            device_name,
            vram_gb,
        },
    };
    (accelerator, devices)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(id: &str, name: &str, vram_gb: f64) -> GpuDevice {
        GpuDevice {
            id: Some(id.to_owned()),
            name: name.to_owned(),
            api: api_of(id),
            vram_gb,
            enabled: true,
        }
    }

    fn two_cards() -> Vec<GpuDevice> {
        vec![
            card("CUDA0", "NVIDIA GeForce RTX 4090", 24.0),
            card("CUDA1", "NVIDIA GeForce RTX 3090", 24.0),
        ]
    }

    fn cuda(vram_gb: f64) -> AcceleratorProfile {
        AcceleratorProfile::Cuda {
            device_name: "NVIDIA GeForce RTX 4090".to_owned(),
            vram_gb,
            compute_capability: (8, 9),
        }
    }

    fn vram_of(accelerator: &AcceleratorProfile) -> Option<f64> {
        match accelerator {
            AcceleratorProfile::None => None,
            AcceleratorProfile::AppleSilicon { vram_gb, .. }
            | AcceleratorProfile::Cuda { vram_gb, .. }
            | AcceleratorProfile::Generic { vram_gb, .. } => Some(*vram_gb),
        }
    }

    #[test]
    fn the_listing_of_the_pinned_engine_yields_its_devices_and_not_its_processor_backend() {
        // GIVEN what `llama-server --list-devices` printed at tag b10092 on an
        // Apple machine, and the same shape for a machine with two cards
        let apple = "Available devices:\n  MTL0: Apple M3 Ultra (228065 MiB, 228064 MiB free)\n  BLAS: Accelerate (0 MiB, 0 MiB free)\n";
        let pc = "Available devices:\n  CUDA0: NVIDIA GeForce RTX 4090 (24564 MiB, 23000 MiB free)\n  CUDA1: NVIDIA GeForce RTX 3090 (24576 MiB, 24000 MiB free)\n";

        // WHEN both are read
        let apple = parse_engine_devices(apple);
        let pc = parse_engine_devices(pc);

        // THEN each graphics device comes with its identifier, its interface
        //      and its memory, and the processor backend is not a device
        assert_eq!(apple.len(), 1);
        assert_eq!(apple[0].id.as_deref(), Some("MTL0"));
        assert_eq!(apple[0].api, "Metal");
        assert_eq!(apple[0].name, "Apple M3 Ultra");
        assert_eq!(
            pc.iter().map(|d| d.id.as_deref()).collect::<Vec<_>>(),
            vec![Some("CUDA0"), Some("CUDA1")]
        );
        assert_eq!(pc[1].api, "CUDA");
        assert!((pc[1].vram_gb - 24.0).abs() < 0.001);
    }

    #[test]
    fn two_cards_left_on_are_sized_by_the_sum_of_their_memory() {
        // GIVEN two 24 GB cards and nothing held back
        // WHEN the selection is folded into the accelerator
        let (accelerator, devices) = select(&cuda(24.0), &two_cards(), &[]);

        // THEN the estimates read 48 GB, and both devices are on
        assert_eq!(vram_of(&accelerator), Some(48.0));
        assert!(devices.iter().all(|d| d.enabled));
    }

    #[test]
    fn a_card_held_back_leaves_the_memory_the_estimates_read() {
        // GIVEN two 24 GB cards, the second held back
        let disabled = vec!["CUDA1".to_owned()];

        // WHEN the selection is folded into the accelerator
        let (accelerator, devices) = select(&cuda(24.0), &two_cards(), &disabled);

        // THEN only the memory of the first card is counted, the card keeps
        //      the kind and the compute capability of the detection, and the
        //      list still shows the second one, off
        assert_eq!(vram_of(&accelerator), Some(24.0));
        assert!(matches!(
            accelerator,
            AcceleratorProfile::Cuda {
                compute_capability: (8, 9),
                ..
            }
        ));
        assert_eq!(
            devices.iter().map(|d| d.enabled).collect::<Vec<_>>(),
            vec![true, false]
        );
    }

    #[test]
    fn every_card_held_back_leaves_a_processor_only_machine() {
        // GIVEN two cards, both held back
        let disabled = vec!["CUDA0".to_owned(), "CUDA1".to_owned()];

        // WHEN the selection is folded into the accelerator
        let (accelerator, devices) = select(&cuda(24.0), &two_cards(), &disabled);

        // THEN there is no accelerator left to size a model on
        assert!(matches!(accelerator, AcceleratorProfile::None));
        assert!(devices.iter().all(|d| !d.enabled));
    }

    #[test]
    fn the_single_device_of_an_apple_machine_is_never_held_back() {
        // GIVEN an Apple machine whose only device a settings file names
        let apple = AcceleratorProfile::AppleSilicon {
            chip: "M3 Ultra".to_owned(),
            generation: 3,
            vram_gb: 256.0,
        };
        let listed = vec![card("MTL0", "Apple M3 Ultra", 222.72)];
        let disabled = vec!["MTL0".to_owned()];

        // WHEN the selection is folded into the accelerator
        let (accelerator, devices) = select(&apple, &listed, &disabled);

        // THEN the accelerator is untouched and the device stays on
        assert_eq!(vram_of(&accelerator), Some(256.0));
        assert_eq!(devices.len(), 1);
        assert!(devices[0].enabled);
    }

    #[test]
    fn without_a_listing_the_detected_card_is_shown_and_cannot_be_held_back() {
        // GIVEN a detected card and an engine that listed nothing
        let disabled = vec!["CUDA0".to_owned()];

        // WHEN the selection is folded into the accelerator
        let (accelerator, devices) = select(&cuda(24.0), &[], &disabled);

        // THEN the card is described from the profile, without the identifier
        //      a switch would need, and it stays counted
        assert_eq!(vram_of(&accelerator), Some(24.0));
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].id, None);
        assert_eq!(devices[0].api, "CUDA");
        assert!(devices[0].enabled);
    }

    #[test]
    fn a_single_card_left_on_keeps_the_figure_of_the_finer_probe() {
        // GIVEN one card, sized 24 GB by the detection and a little less by
        //       the listing
        let listed = vec![card("CUDA0", "NVIDIA GeForce RTX 4090", 23.98)];

        // WHEN nothing is held back
        let (accelerator, _) = select(&cuda(24.0), &listed, &[]);

        // THEN the detected figure stands
        assert_eq!(vram_of(&accelerator), Some(24.0));
    }
}
