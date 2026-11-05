//! Cards as Windows lists its display adapters.
//!
//! One PowerShell query, the same way the accelerator profile used to reach
//! the registry, rather than a WMI dependency this crate does not carry. It
//! prints two kinds of line: each adapter of `Win32_VideoController` with its
//! PCI identifier, and each display adapter of the registry with its memory.
//! The vendor is the `VEN_` field of the PCI identifier. The memory is the
//! registry's `HardwareInformation.qwMemorySize`, a 64-bit value:
//! `Win32_VideoController.AdapterRAM` is a 32-bit field Microsoft never
//! widened, and a 16 GB Radeon reads 4095 MiB there. Nothing maps an AMD card
//! to its gfx target here: no sourced table of PCI identifiers to gfx targets
//! exists, so that target stays unknown rather than guessed from the name.

use std::process::Command;
use std::time::Duration;

use tracing::{event, Level};

use super::{GpuCard, GpuVendor};
use crate::hardware::bounded::output_within;

/// The tag of an adapter line: `adapter<tab>name<tab>PNPDeviceID`.
const ADAPTER: &str = "adapter";
/// The tag of a registry line: `memory<tab>DriverDesc<tab>MatchingDeviceId<tab>bytes`.
const MEMORY: &str = "memory";

// A driver may store the size as an 8-byte binary rather than a QWORD; both
// are turned into a decimal count of bytes.
const SCRIPT: &str = concat!(
    "Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | ",
    "ForEach-Object { 'adapter' + [char]9 + $_.Name + [char]9 + $_.PNPDeviceID }; ",
    r"$k = 'HKLM:\SYSTEM\CurrentControlSet\Control\Class\",
    r"{4d36e968-e325-11ce-bfc1-08002be10318}\*'; ",
    "Get-ItemProperty $k -ErrorAction SilentlyContinue | ForEach-Object { ",
    "$m = $_.'HardwareInformation.qwMemorySize'; ",
    "if ($m -is [byte[]]) { $m = [BitConverter]::ToUInt64($m, 0) }; ",
    "if ($m) { 'memory' + [char]9 + $_.DriverDesc + [char]9 + $_.MatchingDeviceId + [char]9 + $m } }",
);

/// The display adapters of the machine, NVIDIA ones left out when
/// `skip_nvidia`.
pub(super) fn cards(limit: Duration, skip_nvidia: bool) -> Vec<GpuCard> {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    apollia_core::subprocess_window::hide_console(&mut cmd);
    match output_within(&mut cmd, limit) {
        Ok(out) if out.status.success() => parse_listing(&String::from_utf8_lossy(&out.stdout))
            .into_iter()
            .filter(|card| !(skip_nvidia && card.vendor == GpuVendor::Nvidia))
            .collect(),
        Ok(out) => {
            event!(Level::WARN, status = ?out.status.code(), "hardware.adapters_query_refused");
            Vec::new()
        }
        Err(error) => {
            event!(Level::WARN, error = %error, "hardware.adapters_query_failed");
            Vec::new()
        }
    }
}

/// A display adapter as its driver recorded it in the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RegistryAdapter {
    /// `DriverDesc`, the name WMI reports as `Name`.
    driver_desc: String,
    /// `MatchingDeviceId`, lowercase, a prefix of WMI's `PNPDeviceID`.
    matching_device_id: String,
    /// `HardwareInformation.qwMemorySize`, in bytes.
    memory_bytes: u64,
}

/// The cards of the listing [`SCRIPT`] prints, each with the memory of its
/// registry entry. An adapter without a PCI vendor (the basic display driver,
/// a remote session) is not a card.
pub(super) fn parse_listing(listing: &str) -> Vec<GpuCard> {
    let mut adapters = Vec::new();
    let mut registry = Vec::new();
    for line in listing.lines() {
        let mut fields = line.split('\t');
        match fields.next() {
            Some(ADAPTER) => {
                if let (Some(name), Some(pnp)) = (fields.next(), fields.next()) {
                    adapters.push((name.trim().to_owned(), pnp.trim().to_owned()));
                }
            }
            Some(MEMORY) => {
                let (Some(desc), Some(id), Some(bytes)) =
                    (fields.next(), fields.next(), fields.next())
                else {
                    continue;
                };
                if let Ok(memory_bytes) = bytes.trim().parse::<u64>() {
                    if memory_bytes > 0 {
                        registry.push(RegistryAdapter {
                            driver_desc: desc.trim().to_owned(),
                            matching_device_id: id.trim().to_ascii_lowercase(),
                            memory_bytes,
                        });
                    }
                }
            }
            _ => {}
        }
    }
    adapters
        .into_iter()
        .filter_map(|(name, pnp)| {
            let vendor = GpuVendor::from_pci_id(pci_vendor(&pnp)?);
            if name.is_empty() {
                return None;
            }
            let mut card = GpuCard::named(vendor, name);
            card.vram_gb = registry_memory_bytes(&card.name, &pnp, &registry)
                .map(|bytes| bytes as f64 / 1_073_741_824.0);
            Some(card)
        })
        .collect()
}

/// The registry size of the adapter named `name` with the plug-and-play id
/// `pnp_device_id`. The device id decides first, the longest matching id
/// winning, since two cards may share a name and a driver may write a name of
/// its own; the name is the fallback, for an adapter listed without an id.
fn registry_memory_bytes(
    name: &str,
    pnp_device_id: &str,
    adapters: &[RegistryAdapter],
) -> Option<u64> {
    let pnp = pnp_device_id.trim().to_ascii_lowercase();
    let by_id = adapters
        .iter()
        .filter(|a| !a.matching_device_id.is_empty() && pnp.starts_with(&a.matching_device_id))
        .max_by_key(|a| a.matching_device_id.len());
    let name = name.trim();
    by_id
        .or_else(|| {
            adapters
                .iter()
                .find(|a| !name.is_empty() && a.driver_desc.eq_ignore_ascii_case(name))
        })
        .map(|a| a.memory_bytes)
}

/// The vendor of a PNP device identifier: `0x1002` for
/// `PCI\VEN_1002&DEV_73BF&...`.
pub(super) fn pci_vendor(pnp: &str) -> Option<u16> {
    let upper = pnp.trim().to_ascii_uppercase();
    if !upper.starts_with("PCI\\") {
        return None;
    }
    let at = upper.find("VEN_")? + 4;
    let hex = upper.get(at..at + 4)?;
    u16::from_str_radix(hex, 16).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What the query printed on a machine with a 16 GB Radeon, an Intel
    /// integrated card, an NVIDIA card and the basic display driver: the
    /// adapters, then the registry, where the integrated card has no size of
    /// its own and the basic driver no size at all.
    const LISTING: &str = concat!(
        "adapter\tAMD Radeon RX 6900 XT\tPCI\\VEN_1002&DEV_73BF&SUBSYS_0E3A1002&REV_C0\\6&1A2B3C4D&0&00000019\r\n",
        "adapter\tIntel(R) UHD Graphics 770\tPCI\\VEN_8086&DEV_4680&SUBSYS_7D251462&REV_0C\\3&11583659&0&10\r\n",
        "adapter\tNVIDIA GeForce RTX 4070\tPCI\\VEN_10DE&DEV_2786&SUBSYS_51091458&REV_A1\\4&2E7B2A1&0&0008\r\n",
        "adapter\tMicrosoft Basic Display Adapter\tROOT\\BASICDISPLAY\\0000\r\n",
        "memory\tAMD Radeon RX 6900 XT\tpci\\ven_1002&dev_73bf\t17163091968\r\n",
        "memory\tIntel(R) UHD Graphics 770\tPCI\\VEN_8086&DEV_4680\t0\r\n",
        "memory\tNVIDIA GeForce RTX 4070\tpci\\ven_10de&dev_2786\t12884901888\r\n",
        "memory\tMicrosoft Basic Display Adapter\tpci\\cc_0300\t\r\n",
    );

    #[test]
    fn the_vendor_of_each_adapter_is_its_pci_vendor() {
        // GIVEN the listing of a machine with a Radeon, an Intel integrated
        //       card, an NVIDIA card and the basic display driver
        // WHEN it is read
        let cards = parse_listing(LISTING);

        // THEN three cards come back with their vendors, the basic driver is
        //      not one, and the AMD card has no gfx target
        assert_eq!(
            cards.iter().map(|c| c.vendor).collect::<Vec<_>>(),
            vec![GpuVendor::Amd, GpuVendor::Intel, GpuVendor::Nvidia]
        );
        assert_eq!(cards[0].name, "AMD Radeon RX 6900 XT");
        assert_eq!(cards[0].gfx_target, None);
        assert_eq!(cards[0].kfd_accessible, None);
    }

    #[test]
    fn a_sixteen_gigabyte_radeon_reads_as_sixteen_not_four() {
        // GIVEN the recorded listing, where the registry holds the 64-bit
        //       size of the Radeon that `AdapterRAM` saturates at 4095 MiB
        // WHEN it is read
        let cards = parse_listing(LISTING);

        // THEN the Radeon is sized as itself, the NVIDIA card too, and the
        //      integrated card, which has no memory of its own, has none
        let radeon = cards[0].vram_gb.expect("the registry sizes the Radeon");
        assert!((radeon - 15.98).abs() < 0.1, "got {radeon}");
        assert!(radeon > 4.0);
        assert_eq!(cards[1].vram_gb, None);
        let nvidia = cards[2]
            .vram_gb
            .expect("the registry sizes the NVIDIA card");
        assert!((nvidia - 12.0).abs() < 0.01, "got {nvidia}");
    }

    #[test]
    fn the_memory_follows_the_device_id_before_the_name() {
        // GIVEN registry entries for one card under two ids, the longer naming
        //       its subsystem, and an adapter whose name differs from both
        let registry = [
            RegistryAdapter {
                driver_desc: "Generic".to_owned(),
                matching_device_id: "pci\\ven_1002&dev_73bf".to_owned(),
                memory_bytes: 4_294_967_296,
            },
            RegistryAdapter {
                driver_desc: "AMD Radeon RX 6900 XT".to_owned(),
                matching_device_id: "pci\\ven_1002&dev_73bf&subsys_0e3a1002".to_owned(),
                memory_bytes: 17_163_091_968,
            },
        ];
        let pnp = "PCI\\VEN_1002&DEV_73BF&SUBSYS_0E3A1002&REV_C0\\6&1";

        // WHEN the size is looked up by id, by name alone, and for an
        //      adapter the registry does not hold
        let by_id = registry_memory_bytes("Radeon (vendor name)", pnp, &registry);
        let by_name = registry_memory_bytes("amd radeon rx 6900 xt", "", &registry);
        let unknown = registry_memory_bytes(
            "Intel(R) Arc(TM) A770",
            "PCI\\VEN_8086&DEV_56A0\\4&1",
            &registry,
        );

        // THEN the most specific id answers, the description answers when no
        //      id is given, and an unknown adapter has no size
        assert_eq!(by_id, Some(17_163_091_968));
        assert_eq!(by_name, Some(17_163_091_968));
        assert_eq!(unknown, None);
    }

    #[test]
    fn only_a_pci_identifier_carries_a_vendor() {
        // GIVEN identifiers in both cases, and some that are not PCI
        // WHEN their vendor is read
        // THEN `VEN_1002` is AMD whatever the case, and the others have none
        assert_eq!(pci_vendor("PCI\\VEN_1002&DEV_744C"), Some(0x1002));
        assert_eq!(pci_vendor("pci\\ven_1002&dev_744c"), Some(0x1002));
        assert_eq!(pci_vendor("ROOT\\BASICDISPLAY\\0000"), None);
        assert_eq!(pci_vendor("PCI\\VEN_"), None);
    }
}
