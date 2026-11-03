//! NVIDIA cards, as `nvidia-smi` describes them.
//!
//! Two queries, not one. The first asks what every driver answers: name,
//! driver version, memory. The second asks `compute_cap` alone, because no
//! NVIDIA source states which driver introduced that field, and a driver that
//! refuses one field of a query refuses the whole query: asked together, an
//! old driver would hide the card itself.

use std::path::Path;
use std::process::Command;
use std::time::Duration;

use tracing::{event, Level};

use super::{CardProbe, GpuCard, GpuVendor};
use crate::hardware::bounded::{output_within, ProbeRunError};

const INVENTORY: &str = "--query-gpu=name,driver_version,memory.total";
const COMPUTE_CAP: &str = "--query-gpu=compute_cap";
const FORMAT: &str = "--format=csv,noheader,nounits";

/// Why a query gave no usable answer.
enum QueryFailure {
    /// The program is not installed, or exited in error.
    Unanswered,
    /// The program hung past its limit.
    TimedOut,
}

fn query(program: &Path, field: &str, limit: Duration) -> Result<String, QueryFailure> {
    let mut cmd = Command::new(program);
    cmd.args([field, FORMAT]);
    apollia_core::subprocess_window::hide_console(&mut cmd);
    match output_within(&mut cmd, limit) {
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(out) => {
            event!(
                Level::INFO,
                field = field,
                status = ?out.status.code(),
                stderr = %String::from_utf8_lossy(&out.stderr).trim(),
                "hardware.nvidia_smi_refused"
            );
            Err(QueryFailure::Unanswered)
        }
        Err(ProbeRunError::TimedOut { .. }) => Err(QueryFailure::TimedOut),
        Err(error) => {
            if !error.is_not_found() {
                event!(Level::WARN, field = field, error = %error, "hardware.nvidia_smi_failed");
            }
            Err(QueryFailure::Unanswered)
        }
    }
}

/// The NVIDIA cards `program` reports, empty when it is absent or refuses.
pub(super) fn cards(program: &Path, limit: Duration) -> Vec<GpuCard> {
    let inventory = match query(program, INVENTORY, limit) {
        Ok(stdout) => stdout,
        Err(QueryFailure::Unanswered) => return Vec::new(),
        Err(QueryFailure::TimedOut) => {
            let mut card = GpuCard::named(GpuVendor::Nvidia, "NVIDIA GPU".to_owned());
            card.probe = CardProbe::TimedOut;
            return vec![card];
        }
    };
    let caps = query(program, COMPUTE_CAP, limit);
    let timed_out = matches!(caps, Err(QueryFailure::TimedOut));
    let mut cards = combine(&inventory, caps.ok().as_deref());
    if timed_out {
        for card in &mut cards {
            card.probe = CardProbe::TimedOut;
        }
    }
    cards
}

/// The cards of an inventory answer, each given the compute capability on the
/// line of the same rank of `caps`; `None` when that query was refused.
pub(super) fn combine(inventory: &str, caps: Option<&str>) -> Vec<GpuCard> {
    let caps: Vec<Option<(u8, u8)>> = caps
        .map(|caps| caps.lines().map(parse_compute_cap).collect())
        .unwrap_or_default();
    inventory
        .lines()
        .filter_map(parse_inventory_line)
        .enumerate()
        .map(|(rank, mut card)| {
            card.compute_capability = caps.get(rank).copied().flatten();
            card
        })
        .collect()
}

/// One line of the inventory query: `name, driver, memory in MiB`.
fn parse_inventory_line(line: &str) -> Option<GpuCard> {
    // The name comes first and may hold a comma; the two last fields never do.
    let mut fields = line.rsplitn(3, ',').map(str::trim);
    let memory = fields.next()?;
    let driver = fields.next()?;
    let name = fields.next()?;
    if name.is_empty() {
        return None;
    }
    let mut card = GpuCard::named(GpuVendor::Nvidia, name.to_owned());
    card.vram_gb = memory.parse::<f64>().ok().map(|mib| mib / 1024.0);
    card.driver_major = driver_major(driver);
    card.driver_version =
        (!driver.is_empty() && card.driver_major.is_some()).then(|| driver.to_owned());
    Some(card)
}

/// The major part of a driver version: `580` for `580.65.06`.
pub(super) fn driver_major(version: &str) -> Option<u32> {
    version.trim().split('.').next()?.parse().ok()
}

/// A compute capability as `nvidia-smi` prints it, `8.9`.
fn parse_compute_cap(line: &str) -> Option<(u8, u8)> {
    let (major, minor) = line.trim().split_once('.')?;
    Some((major.parse().ok()?, minor.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_card_of_a_two_card_machine_is_listed() {
        // GIVEN the two answers of a machine with two cards
        let inventory = "NVIDIA GeForce RTX 4090, 560.35.03, 24564\nNVIDIA GeForce RTX 3090, 560.35.03, 24576\n";
        let caps = "8.9\n8.6\n";

        // WHEN they are combined
        let cards = combine(inventory, Some(caps));

        // THEN both cards come back, each with its own capability, not the
        //      first card alone as the accelerator profile reads it
        assert_eq!(cards.len(), 2);
        assert_eq!(cards[0].name, "NVIDIA GeForce RTX 4090");
        assert_eq!(cards[0].compute_capability, Some((8, 9)));
        assert_eq!(cards[1].name, "NVIDIA GeForce RTX 3090");
        assert_eq!(cards[1].compute_capability, Some((8, 6)));
        assert!((cards[1].vram_gb.unwrap_or_default() - 24.0).abs() < 0.001);
        assert!(cards.iter().all(|c| c.vendor == GpuVendor::Nvidia));
    }

    #[test]
    fn a_driver_that_refuses_compute_cap_still_lists_its_card() {
        // GIVEN an inventory answer, and a driver that refused the
        //       `compute_cap` query
        let inventory = "Tesla P40, 470.256.02, 24576\n";

        // WHEN they are combined
        let cards = combine(inventory, None);

        // THEN the card is listed with its driver, its capability unknown
        assert_eq!(cards.len(), 1);
        assert_eq!(cards[0].name, "Tesla P40");
        assert_eq!(cards[0].driver_major, Some(470));
        assert_eq!(cards[0].compute_capability, None);
        assert_eq!(cards[0].probe, CardProbe::Complete);
    }

    #[test]
    fn a_three_part_driver_version_reads_as_its_major() {
        // GIVEN the driver of the CUDA 13.0 toolkit under Linux
        let inventory = "NVIDIA GeForce RTX 5090, 580.65.06, 32607\n";

        // WHEN the line is read
        let cards = combine(inventory, Some("12.0\n"));

        // THEN the major is 580, and the full version is kept as printed
        assert_eq!(driver_major("580.65.06"), Some(580));
        assert_eq!(cards[0].driver_major, Some(580));
        assert_eq!(cards[0].driver_version.as_deref(), Some("580.65.06"));
        assert_eq!(cards[0].compute_capability, Some((12, 0)));
    }

    #[test]
    fn a_name_holding_a_comma_is_kept_whole() {
        // GIVEN a card whose name carries a comma
        let inventory = "NVIDIA RTX A6000, Ada, 550.54.14, 49140\n";

        // WHEN the line is read
        let cards = combine(inventory, None);

        // THEN the name keeps its comma and the two last fields stay in place
        assert_eq!(cards[0].name, "NVIDIA RTX A6000, Ada");
        assert_eq!(cards[0].driver_major, Some(550));
    }

    #[cfg(unix)]
    mod processes {
        use super::super::*;
        use crate::hardware::bounded::tests::{fake_tool, ANSWER_LIMIT};

        #[test]
        fn a_driver_refusing_compute_cap_is_asked_separately() {
            // GIVEN a `nvidia-smi` that answers the inventory and refuses the
            //       `compute_cap` field, as a driver predating it would
            let dir = tempfile::tempdir().expect("tempdir");
            let tool = fake_tool(
                dir.path(),
                "nvidia-smi",
                r#"case "$1" in
  *compute_cap*) echo 'Field "compute_cap" is not a valid field to query.' >&2; exit 2;;
  *) echo 'Tesla P40, 470.256.02, 24576';;
esac"#,
            );

            // WHEN the cards are probed
            let cards = cards(&tool, ANSWER_LIMIT);

            // THEN the card is there, capability unknown
            assert_eq!(cards.len(), 1);
            assert_eq!(cards[0].name, "Tesla P40");
            assert_eq!(cards[0].compute_capability, None);
        }

        #[test]
        fn an_absent_nvidia_smi_lists_no_card() {
            // GIVEN no `nvidia-smi` at the path given
            let dir = tempfile::tempdir().expect("tempdir");

            // WHEN the cards are probed
            let cards = cards(&dir.path().join("nvidia-smi"), Duration::from_secs(5));

            // THEN there is none, and nothing is marked as having hung
            assert!(cards.is_empty());
        }
    }
}
