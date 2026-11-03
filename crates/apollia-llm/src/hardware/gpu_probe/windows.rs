//! Cards as Windows lists its display adapters.
//!
//! One PowerShell query of `Win32_VideoController`, the same way the
//! accelerator profile already reaches the registry, rather than a WMI
//! dependency this crate does not carry. The vendor is the `VEN_` field of the
//! PCI identifier. Nothing maps an AMD card to its gfx target here: no sourced
//! table of PCI identifiers to gfx targets exists, so that target stays unknown
//! rather than guessed from the name.

use std::process::Command;
use std::time::Duration;

use tracing::{event, Level};

use super::{GpuCard, GpuVendor};
use crate::hardware::bounded::output_within;

const SCRIPT: &str = concat!(
    "Get-CimInstance Win32_VideoController -ErrorAction SilentlyContinue | ",
    "ForEach-Object { $_.Name + [char]9 + $_.PNPDeviceID }",
);

/// The display adapters of the machine, NVIDIA ones left out when
/// `skip_nvidia`.
pub(super) fn cards(limit: Duration, skip_nvidia: bool) -> Vec<GpuCard> {
    let mut cmd = Command::new("powershell");
    cmd.args(["-NoProfile", "-NonInteractive", "-Command", SCRIPT]);
    apollia_core::subprocess_window::hide_console(&mut cmd);
    match output_within(&mut cmd, limit) {
        Ok(out) if out.status.success() => parse_adapters(&String::from_utf8_lossy(&out.stdout))
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

/// The adapters of a `name<tab>PNPDeviceID` listing. An adapter without a PCI
/// vendor (the basic display driver, a remote session) is not a card.
pub(super) fn parse_adapters(listing: &str) -> Vec<GpuCard> {
    listing
        .lines()
        .filter_map(|line| {
            let (name, pnp) = line.split_once('\t')?;
            let name = name.trim();
            let vendor = GpuVendor::from_pci_id(pci_vendor(pnp)?);
            (!name.is_empty()).then(|| GpuCard::named(vendor, name.to_owned()))
        })
        .collect()
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

    #[test]
    fn the_vendor_of_each_adapter_is_its_pci_vendor() {
        // GIVEN the listing of a machine with a Radeon, an Intel integrated
        //       card, an NVIDIA card and the basic display driver
        let listing = concat!(
            "AMD Radeon RX 6900 XT\tPCI\\VEN_1002&DEV_73BF&SUBSYS_23181458&REV_C0\\6&1A2B3C4D&0&00000019\r\n",
            "Intel(R) UHD Graphics 770\tPCI\\VEN_8086&DEV_4680&SUBSYS_7D251462&REV_0C\\3&11583659&0&10\r\n",
            "NVIDIA GeForce RTX 4070\tPCI\\VEN_10DE&DEV_2786&SUBSYS_51091458&REV_A1\\4&2E7B2A1&0&0008\r\n",
            "Microsoft Basic Display Adapter\tROOT\\BASICDISPLAY\\0000\r\n",
        );

        // WHEN it is read
        let cards = parse_adapters(listing);

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
