//! Cards as the Linux kernel publishes them: `/sys/class/drm` for every card,
//! `/sys/class/kfd` for the gfx target of an AMD one, `/dev/kfd` for whether
//! ROCm can run. File reads only, nothing spawned. Every path is taken under a
//! root so tests read a fake tree.

use std::path::Path;

use super::{GpuCard, GpuVendor};

/// A kfd topology node of a GPU: its gfx target and the render node it drives.
struct KfdNode {
    gfx_target: String,
    render_minor: Option<u32>,
}

/// The cards under `root`, NVIDIA ones left out when `skip_nvidia`.
pub(super) fn cards(root: &Path, skip_nvidia: bool) -> Vec<GpuCard> {
    let drm = root.join("sys/class/drm");
    let Ok(entries) = std::fs::read_dir(&drm) else {
        return Vec::new();
    };
    let mut numbered: Vec<(u32, std::path::PathBuf)> = entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name();
            let index = name.to_str()?.strip_prefix("card")?.parse().ok()?;
            Some((index, entry.path().join("device")))
        })
        .collect();
    numbered.sort_by_key(|(index, _)| *index);

    let nodes = kfd_nodes(root);
    let kfd_accessible = kfd_openable(root);
    let mut cards = Vec::new();
    let mut amd_minors = Vec::new();
    for (_, device) in numbered {
        let Some(vendor_id) = read_hex(&device.join("vendor")) else {
            continue;
        };
        let vendor = GpuVendor::from_pci_id(vendor_id);
        if skip_nvidia && vendor == GpuVendor::Nvidia {
            continue;
        }
        let name = read_trimmed(&device.join("product_name")).unwrap_or_else(|| {
            let device_id = read_trimmed(&device.join("device")).unwrap_or_default();
            format!("{} GPU {device_id}", vendor_label(vendor))
                .trim()
                .to_owned()
        });
        let mut card = GpuCard::named(vendor, name);
        card.vram_gb = read_trimmed(&device.join("mem_info_vram_total"))
            .and_then(|bytes| bytes.parse::<f64>().ok())
            .filter(|bytes| *bytes > 0.0)
            .map(|bytes| bytes / 1_073_741_824.0);
        if vendor == GpuVendor::Amd {
            card.kfd_accessible = Some(kfd_accessible);
            amd_minors.push((cards.len(), render_minor(&device)));
        }
        cards.push(card);
    }

    // Each AMD card takes the gfx target of the kfd node driving its render
    // node. A machine with one AMD card and one GPU node pairs them even when
    // the kernel left the render minor out.
    for (at, minor) in &amd_minors {
        let node = minor
            .and_then(|minor| nodes.iter().find(|n| n.render_minor == Some(minor)))
            .or_else(|| {
                (amd_minors.len() == 1 && nodes.len() == 1)
                    .then(|| nodes.first())
                    .flatten()
            });
        if let (Some(node), Some(card)) = (node, cards.get_mut(*at)) {
            card.gfx_target = Some(node.gfx_target.clone());
        }
    }
    cards
}

fn vendor_label(vendor: GpuVendor) -> &'static str {
    match vendor {
        GpuVendor::Nvidia => "NVIDIA",
        GpuVendor::Amd => "AMD",
        GpuVendor::Intel => "Intel",
        GpuVendor::Other => "",
    }
}

fn read_trimmed(path: &Path) -> Option<String> {
    let text = std::fs::read_to_string(path).ok()?;
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// A sysfs hexadecimal identifier, `0x1002`.
fn read_hex(path: &Path) -> Option<u16> {
    let text = read_trimmed(path)?;
    u16::from_str_radix(text.trim_start_matches("0x"), 16).ok()
}

/// The minor of the render node of a card: `128` for `drm/renderD128`.
fn render_minor(device: &Path) -> Option<u32> {
    std::fs::read_dir(device.join("drm"))
        .ok()?
        .flatten()
        .find_map(|entry| {
            entry
                .file_name()
                .to_str()?
                .strip_prefix("renderD")?
                .parse()
                .ok()
        })
}

/// The GPU nodes of the kfd topology; a processor node has a gfx target of 0.
fn kfd_nodes(root: &Path) -> Vec<KfdNode> {
    let Ok(entries) = std::fs::read_dir(root.join("sys/class/kfd/kfd/topology/nodes")) else {
        return Vec::new();
    };
    let mut nodes: Vec<(String, KfdNode)> = entries
        .flatten()
        .filter_map(|entry| {
            let text = std::fs::read_to_string(entry.path().join("properties")).ok()?;
            let field = |key: &str| {
                text.lines().find_map(|line| {
                    let (k, v) = line.split_once(' ')?;
                    (k == key).then(|| v.trim().parse::<u32>().ok()).flatten()
                })
            };
            let gfx_target = gfx_name(field("gfx_target_version")?)?;
            let order = entry.file_name().to_string_lossy().into_owned();
            Some((
                order,
                KfdNode {
                    gfx_target,
                    render_minor: field("drm_render_minor"),
                },
            ))
        })
        .collect();
    nodes.sort_by(|a, b| a.0.cmp(&b.0));
    nodes.into_iter().map(|(_, node)| node).collect()
}

/// The gfx name of a kfd `gfx_target_version`: major, then minor and stepping
/// as one hexadecimal digit each. `110000` is `gfx1100`, `90010` is `gfx90a`.
pub(super) fn gfx_name(version: u32) -> Option<String> {
    if version == 0 {
        return None;
    }
    let major = version / 10_000;
    let minor = (version / 100) % 100;
    let stepping = version % 100;
    if minor > 15 || stepping > 15 {
        return None;
    }
    Some(format!("gfx{major}{minor:x}{stepping:x}"))
}

/// Whether ROCm's device node opens for reading and writing, as ROCm opens it.
fn kfd_openable(root: &Path) -> bool {
    std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(root.join("dev/kfd"))
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A fake `/sys/class/drm/cardN` under `root`.
    fn drm_card(root: &Path, index: u32, vendor: &str, name: Option<&str>, render: Option<u32>) {
        let device = root.join(format!("sys/class/drm/card{index}/device"));
        fs::create_dir_all(&device).expect("drm dir");
        fs::write(device.join("vendor"), format!("{vendor}\n")).expect("vendor");
        fs::write(device.join("device"), "0x73bf\n").expect("device");
        if let Some(name) = name {
            fs::write(device.join("product_name"), format!("{name}\n")).expect("name");
        }
        if let Some(minor) = render {
            fs::create_dir_all(device.join(format!("drm/renderD{minor}"))).expect("render");
        }
        // A connector entry, which is not a card.
        fs::create_dir_all(root.join(format!("sys/class/drm/card{index}-DP-1")))
            .expect("connector");
    }

    fn kfd_node(root: &Path, node: u32, gfx: u32, render: u32) {
        let dir = root.join(format!("sys/class/kfd/kfd/topology/nodes/{node}"));
        fs::create_dir_all(&dir).expect("kfd dir");
        fs::write(
            dir.join("properties"),
            format!("cpu_cores_count 0\ngfx_target_version {gfx}\ndrm_render_minor {render}\nvendor_id 4098\n"),
        )
        .expect("properties");
    }

    #[test]
    fn an_amd_card_takes_the_gfx_target_of_its_kfd_node() {
        // GIVEN a fake tree with a Radeon on render node 128, the processor
        //       node and the GPU node of the kfd topology, and `/dev/kfd`
        let root = tempfile::tempdir().expect("tempdir");
        drm_card(
            root.path(),
            0,
            "0x1002",
            Some("AMD Radeon RX 7900 XTX"),
            Some(128),
        );
        kfd_node(root.path(), 0, 0, 0);
        kfd_node(root.path(), 1, 110000, 128);
        fs::create_dir_all(root.path().join("dev")).expect("dev");
        fs::write(root.path().join("dev/kfd"), "").expect("kfd");

        // WHEN the cards are read
        let cards = cards(root.path(), false);

        // THEN the card is AMD, named, gfx1100, and ROCm's node opens
        assert_eq!(cards.len(), 1, "the connector entry is not a card");
        assert_eq!(cards[0].vendor, GpuVendor::Amd);
        assert_eq!(cards[0].name, "AMD Radeon RX 7900 XTX");
        assert_eq!(cards[0].gfx_target.as_deref(), Some("gfx1100"));
        assert_eq!(cards[0].kfd_accessible, Some(true));
    }

    #[test]
    fn without_dev_kfd_rocm_is_marked_unreachable() {
        // GIVEN an AMD card and no `/dev/kfd`
        let root = tempfile::tempdir().expect("tempdir");
        drm_card(root.path(), 0, "0x1002", None, None);

        // WHEN the cards are read
        let cards = cards(root.path(), false);

        // THEN the card is listed, its gfx target unknown, the node closed
        assert_eq!(cards[0].vendor, GpuVendor::Amd);
        assert_eq!(cards[0].name, "AMD GPU 0x73bf");
        assert_eq!(cards[0].gfx_target, None);
        assert_eq!(cards[0].kfd_accessible, Some(false));
    }

    #[test]
    fn two_amd_cards_each_take_their_own_node() {
        // GIVEN two AMD cards whose kfd nodes are listed in the other order
        let root = tempfile::tempdir().expect("tempdir");
        drm_card(root.path(), 0, "0x1002", Some("first"), Some(128));
        drm_card(root.path(), 1, "0x1002", Some("second"), Some(129));
        kfd_node(root.path(), 1, 100300, 129);
        kfd_node(root.path(), 2, 90010, 128);

        // WHEN the cards are read
        let cards = cards(root.path(), false);

        // THEN each card has the target of the node driving its render node
        assert_eq!(cards[0].gfx_target.as_deref(), Some("gfx90a"));
        assert_eq!(cards[1].gfx_target.as_deref(), Some("gfx1030"));
    }

    #[test]
    fn nvidia_and_intel_cards_are_told_apart_and_nvidia_skipped_on_demand() {
        // GIVEN an Intel card and an NVIDIA one
        let root = tempfile::tempdir().expect("tempdir");
        drm_card(root.path(), 0, "0x8086", None, None);
        drm_card(root.path(), 1, "0x10de", None, None);

        // WHEN the cards are read with and without the NVIDIA ones
        let all = cards(root.path(), false);
        let others = cards(root.path(), true);

        // THEN both vendors are read, and the NVIDIA card can be left to
        //      `nvidia-smi`
        assert_eq!(
            all.iter().map(|c| c.vendor).collect::<Vec<_>>(),
            vec![GpuVendor::Intel, GpuVendor::Nvidia]
        );
        assert_eq!(others.len(), 1);
        assert_eq!(others[0].vendor, GpuVendor::Intel);
        assert_eq!(others[0].kfd_accessible, None);
    }

    #[test]
    fn gfx_target_versions_convert_to_their_names() {
        // GIVEN the targets of the ROCm builds of the engine
        // WHEN their kfd versions are converted
        // THEN the names come out as ROCm spells them, and a processor node
        //      has none
        assert_eq!(gfx_name(110000).as_deref(), Some("gfx1100"));
        assert_eq!(gfx_name(90402).as_deref(), Some("gfx942"));
        assert_eq!(gfx_name(120001).as_deref(), Some("gfx1201"));
        assert_eq!(gfx_name(110501).as_deref(), Some("gfx1151"));
        assert_eq!(gfx_name(0), None);
    }

    #[test]
    fn a_tree_without_sysfs_lists_nothing() {
        // GIVEN an empty root
        let root = tempfile::tempdir().expect("tempdir");

        // WHEN the cards are read
        // THEN there is none
        assert!(cards(root.path(), false).is_empty());
    }
}
