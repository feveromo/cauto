use std::ffi::OsString;
use std::sync::Arc;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

use cauto::routing::{
    BoundedScore, CapabilitySource, Conflict, DimensionScores, ModelFamily, ReasoningLevel,
    RouteSource, TaskType,
};
use cauto::state::decision_log::{DecisionRecord, append_json_line, latest_route, timestamp_now};
use cauto::state::{FeedbackKind, append_feedback};
use cauto::state::{build_report, prompt_sha256, sanitize_argv};
use proptest::prelude::*;
use tempfile::tempdir;

fn record(index: usize) -> DecisionRecord {
    let one = BoundedScore::new(1).unwrap();
    DecisionRecord {
        schema_version: 1,
        record_type: "decision".into(),
        decision_mode: "launched".into(),
        decision_id: format!("id-{index}"),
        timestamp: timestamp_now(),
        cauto_version: "test".into(),
        codex_version: "test".into(),
        repository_identifier: "repo".into(),
        repository_name: "repo".into(),
        git_branch: Some("main".into()),
        prompt_sha256: prompt_sha256(OsString::from(format!("secret-{index}")).as_os_str()),
        prompt_byte_length: 8,
        task_type: TaskType::Coding,
        dimensions: DimensionScores {
            scope: one,
            ambiguity: one,
            cost_of_being_wrong: one,
            runtime_dependence: one,
            architectural_depth: one,
            verification_burden: one,
            parallelizability: one,
        },
        complexity_score: 25,
        calibration: None,
        confidence_basis_points: 8_000,
        matched_rule_ids: vec!["raise".into()],
        raising_rule_ids: vec!["raise".into()],
        lowering_rule_ids: vec![],
        conflicts: Vec::<Conflict>::new(),
        selected_model: "gpt-5.6-terra".into(),
        selected_family: ModelFamily::Terra,
        selected_effort: ReasoningLevel::Medium,
        ultra_candidate: false,
        ultra_selected: false,
        route_source: RouteSource::Local,
        routing_elapsed_micros: 100 + index as u64,
        classifier_ran: index.is_multiple_of(2),
        classifier_outcome: "success".into(),
        catalog_source: CapabilitySource::Cache,
        downgrade: None,
        sanitized_argv: vec!["--model".into(), "gpt-5.6-terra".into()],
        feedback: None,
    }
}

#[test]
fn raw_prompt_is_absent_from_serialized_record() {
    let raw = "extremely secret prompt";
    let mut record = record(0);
    record.prompt_sha256 = prompt_sha256(OsString::from(raw).as_os_str());
    let json = serde_json::to_string(&record).unwrap();
    assert!(!json.contains(raw));
    assert!(json.contains(&record.prompt_sha256));
}

#[test]
fn unknown_config_values_are_redacted_from_argv() {
    let args = [
        OsString::from("-c"),
        OsString::from("model_reasoning_effort=\"high\""),
        OsString::from("--config"),
        OsString::from("mcp.secret=\"do-not-log\""),
    ];
    let sanitized = sanitize_argv(&args);
    assert!(sanitized.iter().any(|value| value.contains("high")));
    assert!(!sanitized.iter().any(|value| value.contains("do-not-log")));
}

#[test]
fn concurrent_loggers_append_complete_json_lines() {
    let root = tempdir().unwrap();
    let path = Arc::new(root.path().join("decisions.jsonl"));
    let mut workers = Vec::new();
    for index in 0..16 {
        let path = Arc::clone(&path);
        workers.push(std::thread::spawn(move || {
            let bytes = serde_json::to_vec(&record(index)).unwrap();
            append_json_line(&path, &bytes).unwrap();
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    let contents = std::fs::read_to_string(path.as_ref()).unwrap();
    assert_eq!(contents.lines().count(), 16);
    for line in contents.lines() {
        serde_json::from_str::<DecisionRecord>(line).unwrap();
    }
}

#[cfg(unix)]
#[test]
fn existing_decision_log_permissions_are_tightened() {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");
    std::fs::write(&path, b"existing\n").unwrap();
    let mut permissions = path.metadata().unwrap().permissions();
    permissions.set_mode(0o666);
    std::fs::set_permissions(&path, permissions).unwrap();

    append_json_line(&path, b"next").unwrap();

    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
}

#[test]
fn report_summarizes_routes_and_rules() {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");
    for index in 0..4 {
        let bytes = serde_json::to_vec(&record(index)).unwrap();
        append_json_line(&path, &bytes).unwrap();
    }
    append_json_line(
        &path,
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "record_type": "feedback",
            "decision_id": "id-0",
            "repository_identifier": "repo",
            "feedback": "right",
            "source": "implicit-correction"
        }))
        .unwrap(),
    )
    .unwrap();
    let report = build_report(&path).unwrap();
    assert_eq!(report.total_decisions, 4);
    assert_eq!(report.total_launched_decisions, 4);
    assert_eq!(report.total_preview_decisions, 0);
    assert_eq!(report.total_legacy_decisions, 0);
    assert_eq!(report.average_confidence_basis_points, 8_000);
    assert_eq!(report.legacy_classifier_sample_count, 4);
    assert_eq!(report.legacy_classifier_invocation_rate_basis_points, 5_000);
    assert_eq!(report.rules_most_often_raising_effort[0].0, "raise");
    assert_eq!(report.feedback_by_route["terra:medium"]["right"], 1);
    assert_eq!(
        report.feedback_source_distribution["implicit-correction"],
        1
    );
    assert_eq!(
        report.feedback_by_repository[0].repository_identifier,
        "repo"
    );
    assert_eq!(report.feedback_by_repository[0].feedback.right, 1);
    assert_eq!(
        report.feedback_by_repository[0].status,
        "insufficient-feedback"
    );
}

#[test]
fn missing_history_returns_an_empty_report() {
    let root = tempdir().unwrap();
    let report = build_report(&root.path().join("missing.jsonl")).unwrap();
    assert_eq!(report.total_decisions, 0);
    assert_eq!(report.total_launched_decisions, 0);
}

#[test]
fn report_separates_launched_preview_and_legacy_decisions() {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");

    let launched = record(1);
    append_json_line(&path, &serde_json::to_vec(&launched).unwrap()).unwrap();

    let mut preview = record(2);
    preview.decision_mode = "preview".into();
    preview.selected_family = ModelFamily::Sol;
    preview.selected_effort = ReasoningLevel::Max;
    append_json_line(&path, &serde_json::to_vec(&preview).unwrap()).unwrap();

    let mut agent = record(4);
    agent.decision_mode = "agent".into();
    agent.selected_family = ModelFamily::Sol;
    agent.selected_effort = ReasoningLevel::High;
    append_json_line(&path, &serde_json::to_vec(&agent).unwrap()).unwrap();

    let mut legacy = serde_json::to_value(record(3)).unwrap();
    legacy.as_object_mut().unwrap().remove("decision_mode");
    legacy["selected_family"] = serde_json::json!("luna");
    legacy["selected_effort"] = serde_json::json!("low");
    append_json_line(&path, &serde_json::to_vec(&legacy).unwrap()).unwrap();

    let report = build_report(&path).unwrap();
    assert_eq!(report.schema_version, 4);
    assert_eq!(report.total_decisions, 4);
    assert_eq!(report.total_launched_decisions, 2);
    assert_eq!(report.total_agent_decisions, 1);
    assert_eq!(report.total_preview_decisions, 1);
    assert_eq!(report.total_legacy_decisions, 1);
    assert_eq!(report.route_distribution["terra:medium"], 1);
    assert_eq!(report.route_distribution["sol:high"], 1);
    assert_eq!(report.agent_route_distribution["sol:high"], 1);
    assert_eq!(report.preview_route_distribution["sol:max"], 1);
    assert_eq!(report.legacy_route_distribution["luna:low"], 1);
    assert_eq!(report.route_source_distribution["local"], 2);
}

#[test]
fn report_tracks_native_preservation_and_local_routing_latency() {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");
    for (index, elapsed) in [10, 20, 30, 400].into_iter().enumerate() {
        let mut decision = record(index);
        decision.schema_version = 2;
        decision.decision_mode = "agent".into();
        decision.routing_elapsed_micros = elapsed;
        if index == 3 {
            decision.route_source = RouteSource::NativePreserved;
        }
        append_json_line(&path, &serde_json::to_vec(&decision).unwrap()).unwrap();
    }

    let report = build_report(&path).unwrap();
    assert_eq!(report.route_source_distribution["local"], 3);
    assert_eq!(report.route_source_distribution["native-preserved"], 1);
    assert_eq!(report.agent_native_preserved_rate_basis_points, 2_500);
    assert_eq!(report.routing_latency_micros.sample_count, 4);
    assert_eq!(report.routing_latency_micros.p50, 20);
    assert_eq!(report.routing_latency_micros.p95, 400);
    assert_eq!(report.routing_latency_micros.max, 400);
    assert_eq!(report.legacy_classifier_sample_count, 0);
}

#[test]
fn report_surfaces_unrecognized_generic_baseline_launches() {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");
    let mut generic = record(1);
    generic.dimensions = DimensionScores::default();
    generic.complexity_score = 21;
    generic.matched_rule_ids.clear();
    append_json_line(&path, &serde_json::to_vec(&generic).unwrap()).unwrap();

    let mut semantically_classified = record(2);
    semantically_classified.dimensions = DimensionScores::default();
    semantically_classified.complexity_score = 21;
    semantically_classified.matched_rule_ids.clear();
    semantically_classified.classifier_ran = true;
    semantically_classified.classifier_outcome = "success".into();
    append_json_line(
        &path,
        &serde_json::to_vec(&semantically_classified).unwrap(),
    )
    .unwrap();

    let report = build_report(&path).unwrap();
    assert_eq!(report.unresolved_generic_baseline_decisions, 1);
    assert_eq!(report.unresolved_generic_baseline_rate_basis_points, 5_000);
}

#[test]
fn latest_effort_is_bounded_to_the_requested_repository() {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");
    let mut other = record(1);
    other.repository_identifier = "other".into();
    other.selected_effort = ReasoningLevel::Max;
    append_json_line(&path, &serde_json::to_vec(&other).unwrap()).unwrap();
    let mut current = record(2);
    current.selected_effort = ReasoningLevel::High;
    append_json_line(&path, &serde_json::to_vec(&current).unwrap()).unwrap();
    append_json_line(&path, b"not-json").unwrap();

    assert_eq!(
        latest_route(&path, "repo").unwrap(),
        Some((ModelFamily::Terra, ReasoningLevel::High))
    );
    assert_eq!(
        latest_route(&path, "other").unwrap(),
        Some((ModelFamily::Terra, ReasoningLevel::Max))
    );
    assert_eq!(latest_route(&path, "missing").unwrap(), None);
}

#[test]
fn preview_decisions_do_not_affect_hysteresis() {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");
    let mut preview = record(1);
    preview.decision_mode = "preview".into();
    preview.selected_effort = ReasoningLevel::Max;
    append_json_line(&path, &serde_json::to_vec(&preview).unwrap()).unwrap();
    let mut launched = record(2);
    launched.selected_effort = ReasoningLevel::Medium;
    append_json_line(&path, &serde_json::to_vec(&launched).unwrap()).unwrap();

    assert_eq!(
        latest_route(&path, "repo").unwrap(),
        Some((ModelFamily::Terra, ReasoningLevel::Medium))
    );
}

#[test]
fn feedback_ignores_preview_decisions() {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");
    let mut launched = record(1);
    launched.decision_id = "launched".into();
    append_json_line(&path, &serde_json::to_vec(&launched).unwrap()).unwrap();
    let mut preview = record(2);
    preview.decision_id = "preview".into();
    preview.decision_mode = "preview".into();
    append_json_line(&path, &serde_json::to_vec(&preview).unwrap()).unwrap();

    assert_eq!(
        append_feedback(&path, "repo", FeedbackKind::Underpowered).unwrap(),
        "launched"
    );
}

#[test]
fn feedback_ignores_native_preserved_decisions() {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");
    let mut local = record(1);
    local.decision_id = "local".into();
    append_json_line(&path, &serde_json::to_vec(&local).unwrap()).unwrap();
    let mut native = record(2);
    native.decision_id = "native".into();
    native.route_source = RouteSource::NativePreserved;
    append_json_line(&path, &serde_json::to_vec(&native).unwrap()).unwrap();

    assert_eq!(
        append_feedback(&path, "repo", FeedbackKind::Underpowered).unwrap(),
        "local"
    );
}

proptest! {
    #[test]
    fn arbitrary_raw_prompt_never_appears_in_record(prompt in "[A-Z]{24,64}") {
        let mut record = record(99);
        record.prompt_sha256 = prompt_sha256(OsString::from(&prompt).as_os_str());
        let json = serde_json::to_string(&record).unwrap();
        prop_assert!(!json.contains(&prompt));
    }
}
