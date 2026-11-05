//! Whether each installed GPU build lists a device, asked under one deadline.
//!
//! Asking a build spawns it, and a build or a driver in a bad state can hang.
//! Each ask is bounded by the probe limit; on top of that, a rating of the
//! machine asks every build at once and waits at most [`RATING_DEADLINE`] for
//! all of them and for the hardware profile, the deadline of the engine list.
//! A build that has not answered by then is taken as not asked, and kept so:
//! every answer, a failure or a silence included, is kept for the process
//! until the channel check forgets them, so a hung build costs one deadline,
//! not one per launch.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::sync::RwLock;
use std::time::{Duration, Instant};

use apollia_llm::hardware::bounded::PROBE_LIMIT;
use apollia_llm::hardware::GpuProfile;

use crate::engine_store::Variant;

/// What a rating of the machine waits, at most, for the hardware probes: the
/// limit of one probe and a second to collect them.
pub(crate) const RATING_DEADLINE: Duration = PROBE_LIMIT.saturating_add(Duration::from_secs(1));

/// The answers of the builds, by binary path: `Some` whether it lists a
/// device, `None` when it could not be run or did not answer in time.
///
/// A cache of probe results, like the hardware profile beside it, that no
/// actor shares state through.
pub(crate) struct Listings(RwLock<BTreeMap<PathBuf, Option<bool>>>);

impl Listings {
    pub(crate) const fn new() -> Self {
        Self(RwLock::new(BTreeMap::new()))
    }

    /// The kept answer of `binary`, the outer `None` when it was never asked.
    fn kept(&self, binary: &Path) -> Option<Option<bool>> {
        self.0.read().ok().and_then(|m| m.get(binary).copied())
    }

    fn keep(&self, binary: &Path, answer: Option<bool>) {
        if let Ok(mut m) = self.0.write() {
            m.insert(binary.to_path_buf(), answer);
        }
    }

    /// Drop every kept answer, so the next rating asks each build again.
    pub(crate) fn forget(&self) {
        if let Ok(mut m) = self.0.write() {
            m.clear();
        }
    }
}

/// The answers of this process.
pub(crate) static LISTINGS: Listings = Listings::new();

/// Where a rating reads the machine: the probes of this machine, or fakes.
#[derive(Clone, Copy)]
pub(crate) struct Probes {
    /// The kept hardware profile, probed on first use. Blocking.
    pub(crate) profile: fn() -> GpuProfile,
    /// Whether the build at a path lists a device, unkept. Blocking.
    pub(crate) listing: fn(&Path) -> Option<bool>,
}

/// The probes of this machine.
pub(crate) const SYSTEM: Probes = Probes {
    profile: apollia_llm::hardware::gpu_profile,
    listing: |binary| listing_within(binary, PROBE_LIMIT),
};

/// Whether the build at `binary` lists a device with `--list-devices`, asked
/// under `limit`. `None` when the build could not be run: a build that does
/// not start is not a build that sees nothing.
pub(crate) fn listing_within(binary: &Path, limit: Duration) -> Option<bool> {
    match apollia_llm::hardware::devices::list_devices_within(binary, limit) {
        Ok(devices) => Some(!devices.is_empty()),
        Err(error) => {
            tracing::event!(
                tracing::Level::WARN,
                binary = %binary.display(),
                error = %error,
                "engine_store.list_devices.failed"
            );
            None
        }
    }
}

/// [`listing_within`] under the probe limit, kept in `cache`.
pub(crate) fn kept_listing(cache: &Listings, binary: &Path) -> Option<bool> {
    if let Some(known) = cache.kept(binary) {
        return known;
    }
    let answer = listing_within(binary, PROBE_LIMIT);
    cache.keep(binary, answer);
    answer
}

/// One answer of a probe run on its own thread.
enum Answer {
    Profile(GpuProfile),
    Listing(PathBuf, Option<bool>),
}

/// What the probes gave within the deadline.
pub(crate) struct Facts {
    /// The profile, `None` when it was not probed in time.
    pub(crate) profile: Option<GpuProfile>,
    /// Per GPU variant of `builds`, whether its build lists a device; absent
    /// when it could not be run or did not answer in time.
    pub(crate) listings: HashMap<Variant, bool>,
}

/// Probe the profile through `probes` and ask each of `builds` not yet in
/// `cache` whether it lists a device, all at once, waiting `deadline` at
/// most. Every answer is kept in `cache`, a build late or failed as `None`.
///
/// Blocking, for `deadline` at most: call from `spawn_blocking`. A probe left
/// running ends on its own limit; its late answer is dropped.
pub(crate) fn facts_within(
    builds: Vec<(Variant, PathBuf)>,
    probes: Probes,
    cache: &Listings,
    deadline: Duration,
) -> Facts {
    let until = Instant::now() + deadline;
    let (tx, rx) = mpsc::sync_channel(builds.len().saturating_add(1));
    let mut waiting = usize::from(run_apart("profile", tx.clone(), move || {
        Answer::Profile((probes.profile)())
    }));
    let mut asked: HashMap<PathBuf, Variant> = HashMap::new();
    let mut listings = HashMap::new();
    for (variant, binary) in builds {
        match cache.kept(&binary) {
            Some(known) => {
                if let Some(sees) = known {
                    listings.insert(variant, sees);
                }
            }
            None => {
                let path = binary.clone();
                if run_apart("listing", tx.clone(), move || {
                    let answer = (probes.listing)(&path);
                    Answer::Listing(path, answer)
                }) {
                    waiting += 1;
                }
                asked.insert(binary, variant);
            }
        }
    }
    drop(tx);
    let mut profile = None;
    while waiting > 0 {
        let left = until.saturating_duration_since(Instant::now());
        let Ok(answer) = rx.recv_timeout(left) else {
            break;
        };
        waiting -= 1;
        match answer {
            Answer::Profile(p) => profile = Some(p),
            Answer::Listing(binary, answer) => {
                if let Some(variant) = asked.remove(&binary) {
                    cache.keep(&binary, answer);
                    if let Some(sees) = answer {
                        listings.insert(variant, sees);
                    }
                }
            }
        }
    }
    for binary in asked.into_keys() {
        tracing::event!(
            tracing::Level::WARN,
            binary = %binary.display(),
            deadline_ms = deadline.as_millis(),
            "engine_store.list_devices.late"
        );
        cache.keep(&binary, None);
    }
    if profile.is_none() {
        tracing::event!(
            tracing::Level::WARN,
            deadline_ms = deadline.as_millis(),
            "engine_store.ratings.profile_late"
        );
    }
    Facts { profile, listings }
}

/// Run `probe` on a thread of its own, its answer sent on `tx`. False when
/// the thread could not be started, which leaves that probe unanswered.
fn run_apart(
    what: &'static str,
    tx: mpsc::SyncSender<Answer>,
    probe: impl FnOnce() -> Answer + Send + 'static,
) -> bool {
    let started = std::thread::Builder::new()
        .name(format!("engine-probe-{what}"))
        .spawn(move || {
            let _ = tx.send(probe());
        });
    if let Err(error) = &started {
        tracing::event!(
            tracing::Level::WARN,
            probe = what,
            error = %error,
            "engine_store.probe.not_started"
        );
    }
    started.is_ok()
}

#[cfg(test)]
mod tests;
