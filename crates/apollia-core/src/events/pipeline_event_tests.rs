use super::*;

/// Serialize / deserialize roundtrip of `PipelineStarted`.
#[test]
fn test_pipeline_started_roundtrip() {
    // GIVEN
    let event = RuntimeEvent::PipelineStarted {
        run_id: "r-0017".into(),
        pipeline_id: "traitement-facture".into(),
        trigger_id: Some("factures-auto".into()),
        step_count: 6,
    };
    // WHEN
    let json = serde_json::to_string(&event).unwrap();
    let restored: RuntimeEvent = serde_json::from_str(&json).unwrap();
    // THEN
    assert!(matches!(
        restored,
        RuntimeEvent::PipelineStarted { step_count: 6, .. }
    ));
}

/// Serialize / deserialize roundtrip of `PipelineCompleted`.
#[test]
fn test_pipeline_completed_roundtrip() {
    // GIVEN
    let event = RuntimeEvent::PipelineCompleted {
        run_id: "r-0017".into(),
        pipeline_id: "traitement-facture".into(),
        duration_ms: 9400,
    };
    // WHEN
    let json = serde_json::to_string(&event).unwrap();
    let restored: RuntimeEvent = serde_json::from_str(&json).unwrap();
    // THEN
    assert!(matches!(
        restored,
        RuntimeEvent::PipelineCompleted {
            duration_ms: 9400,
            ..
        }
    ));
}

/// Serialize / deserialize roundtrip of `PipelineStepSkipped`.
#[test]
fn test_pipeline_step_skipped_roundtrip() {
    // GIVEN
    let event = RuntimeEvent::PipelineStepSkipped {
        run_id: "r-0017".into(),
        step_id: "alerte-fraude".into(),
        reason: "condition=false".into(),
    };
    // WHEN
    let json = serde_json::to_string(&event).unwrap();
    let restored: RuntimeEvent = serde_json::from_str(&json).unwrap();
    // THEN
    assert!(matches!(restored, RuntimeEvent::PipelineStepSkipped { .. }));
}

/// All 9 Pipeline variants are constructible (zero compilation warning).
#[test]
fn test_all_pipeline_events_compile() {
    // GIVEN / WHEN: construct each variant
    let events: Vec<RuntimeEvent> = vec![
        RuntimeEvent::PipelineStarted {
            run_id: "r".into(),
            pipeline_id: "p".into(),
            trigger_id: None,
            step_count: 1,
        },
        RuntimeEvent::PipelineStepStarted {
            run_id: "r".into(),
            step_id: "s".into(),
            task_id: "t".into(),
            agent: "a".into(),
        },
        RuntimeEvent::PipelineStepCompleted {
            run_id: "r".into(),
            step_id: "s".into(),
        },
        RuntimeEvent::PipelineStepFailed {
            run_id: "r".into(),
            step_id: "s".into(),
            reason: "err".into(),
            on_failure: "fail".into(),
        },
        RuntimeEvent::PipelineStepSkipped {
            run_id: "r".into(),
            step_id: "s".into(),
            reason: "condition=false".into(),
        },
        RuntimeEvent::PipelineSuspended {
            run_id: "r".into(),
            step_id: "s".into(),
            task_id: "t".into(),
        },
        RuntimeEvent::PipelineResumed {
            run_id: "r".into(),
            step_id: "s".into(),
        },
        RuntimeEvent::PipelineCompleted {
            run_id: "r".into(),
            pipeline_id: "p".into(),
            duration_ms: 1000,
        },
        RuntimeEvent::PipelineFailed {
            run_id: "r".into(),
            pipeline_id: "p".into(),
            step_id: "s".into(),
            reason: "err".into(),
        },
    ];
    // THEN
    assert_eq!(events.len(), 9);
}
