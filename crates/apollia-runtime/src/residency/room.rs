//! Making room for a model under the load guardrail when others are resident.
//!
//! How many models stay loaded is not a fixed number: it is what the load
//! guardrail admits. A model is judged with the footprint of the models
//! already resident added to its own. When that sum is refused, the resident
//! models nothing holds (no open conversation, no pin, no request in progress)
//! are candidates to give their memory back, least recently used first, and
//! only as many as the new model needs. When even all of them would not make
//! it fit, none is stopped and the load is refused as it was.
//!
//! An explicit ceiling, `APOLLIA_LLAMA_MAX_LOADED`, still applies after this
//! judgement when it is set.

use crate::local_engine::guardrails::{decide, GuardrailPolicy, GuardrailVerdict, LoadEstimate};

/// A model resident in an engine, as the guardrail weighs it.
#[derive(Debug, Clone, PartialEq)]
pub struct Resident {
    /// Path it was loaded from.
    pub model_path: String,
    /// Its estimated footprint, in gigabytes; `0` when it was not estimated.
    pub footprint_gb: f64,
    /// Whether it may be unloaded to make room: no open conversation, no pin,
    /// no request in progress.
    pub evictable: bool,
    /// Tick of its last use, which orders the candidates.
    pub last_used: u64,
}

/// What loading a model takes, with the models already resident.
#[derive(Debug, Clone, PartialEq)]
pub enum Room {
    /// The model fits beside the resident ones.
    Fits,
    /// The model fits once these resident models are unloaded.
    Evict(Vec<String>),
    /// The model does not fit, even with every candidate unloaded.
    Refused,
}

/// `estimate` with `resident_gb` more already in memory: the footprints add
/// up, and a sum past what the machine holds grades as too large.
#[must_use]
pub fn with_residents(estimate: &LoadEstimate, resident_gb: f64) -> LoadEstimate {
    let total_gb = estimate.total_gb + resident_gb.max(0.0);
    let badge = if resident_gb > 0.0 && total_gb > estimate.capacity_gb {
        apollia_llm::hardware::CompatibilityBadge::TooLarge
    } else {
        estimate.badge
    };
    LoadEstimate {
        total_gb,
        capacity_gb: estimate.capacity_gb,
        badge,
    }
}

/// What loading the model `estimate` sizes takes under `policy`, beside
/// `residents`, and the estimate the guardrail must then judge.
#[must_use]
pub fn plan_room(
    policy: &GuardrailPolicy,
    estimate: &LoadEstimate,
    residents: &[Resident],
) -> (Room, LoadEstimate) {
    let resident_gb: f64 = residents.iter().map(|r| r.footprint_gb).sum();
    let combined = with_residents(estimate, resident_gb);
    if decide(policy, &combined) != GuardrailVerdict::Refused {
        return (Room::Fits, combined);
    }
    let mut candidates: Vec<&Resident> = residents.iter().filter(|r| r.evictable).collect();
    candidates.sort_by_key(|r| r.last_used);
    let mut freed_gb = 0.0;
    let mut evicted = Vec::new();
    for candidate in candidates {
        freed_gb += candidate.footprint_gb;
        evicted.push(candidate.model_path.clone());
        let after = with_residents(estimate, resident_gb - freed_gb);
        if decide(policy, &after) != GuardrailVerdict::Refused {
            return (Room::Evict(evicted), after);
        }
    }
    (Room::Refused, combined)
}

#[cfg(test)]
mod tests;
