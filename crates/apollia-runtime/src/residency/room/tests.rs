use apollia_llm::hardware::CompatibilityBadge;

use super::*;
use crate::local_engine::guardrails::GuardrailLevel;

fn ceiling(gb: f64) -> GuardrailPolicy {
    GuardrailPolicy {
        level: GuardrailLevel::Ceiling,
        ceiling_gb: Some(gb),
    }
}

fn model(gb: f64) -> LoadEstimate {
    LoadEstimate {
        total_gb: gb,
        capacity_gb: 16.0,
        badge: CompatibilityBadge::Fits,
    }
}

fn resident(path: &str, gb: f64, evictable: bool, last_used: u64) -> Resident {
    Resident {
        model_path: path.to_owned(),
        footprint_gb: gb,
        evictable,
        last_used,
    }
}

#[test]
fn test_two_models_that_fit_stay_loaded_together() {
    // GIVEN a 10 GB ceiling and a 4 GB model resident
    let residents = [resident("/a", 4.0, true, 1)];

    // WHEN a second 4 GB model is judged
    let (room, judged) = plan_room(&ceiling(10.0), &model(4.0), &residents);

    // THEN it fits beside the first, judged at their sum
    assert_eq!(room, Room::Fits);
    assert!((judged.total_gb - 8.0).abs() < 1e-9);
}

#[test]
fn test_a_third_model_that_does_not_fit_beside_held_ones_is_refused() {
    // GIVEN a 10 GB ceiling and two 4 GB models conversations hold
    let residents = [resident("/a", 4.0, false, 1), resident("/b", 4.0, false, 2)];

    // WHEN a third 4 GB model is judged
    let (room, judged) = plan_room(&ceiling(10.0), &model(4.0), &residents);

    // THEN it is refused, judged at the sum of all three, and nothing is
    //      stopped
    assert_eq!(room, Room::Refused);
    assert!((judged.total_gb - 12.0).abs() < 1e-9);
}

#[test]
fn test_room_is_made_from_the_least_recently_used_model_nothing_holds_and_no_more() {
    // GIVEN a 10 GB ceiling, a held model and two free ones, the oldest last
    //       used at tick 1
    let residents = [
        resident("/held", 3.0, false, 0),
        resident("/newer", 3.0, true, 5),
        resident("/older", 3.0, true, 1),
    ];

    // WHEN a 4 GB model is judged
    let (room, judged) = plan_room(&ceiling(10.0), &model(4.0), &residents);

    // THEN the oldest free model alone is stopped
    assert_eq!(room, Room::Evict(vec!["/older".to_owned()]));
    assert!((judged.total_gb - 10.0).abs() < 1e-9);
}

#[test]
fn test_no_model_is_stopped_when_even_all_candidates_would_not_make_room() {
    // GIVEN a 10 GB ceiling and one free 2 GB model resident
    let residents = [resident("/a", 2.0, true, 1)];

    // WHEN a 12 GB model is judged
    let (room, _) = plan_room(&ceiling(10.0), &model(12.0), &residents);

    // THEN it is refused and the resident one left alone
    assert_eq!(room, Room::Refused);
}

#[test]
fn test_residents_past_the_machine_grade_the_sum_too_large_under_balanced() {
    // GIVEN the balanced level, a 16 GB machine and 10 GB held in memory
    let residents = [resident("/a", 10.0, false, 1)];

    // WHEN an 8 GB model that fits alone is judged
    let (room, judged) = plan_room(&GuardrailPolicy::default(), &model(8.0), &residents);

    // THEN the sum is too large and the load refused
    assert_eq!(room, Room::Refused);
    assert_eq!(judged.badge, CompatibilityBadge::TooLarge);
}
