use std::ffi::OsString;
use std::sync::Arc;

use cauto::routing::{
    BoundedScore, CapabilitySource, Conflict, DimensionScores, ModelFamily, ReasoningLevel,
    TaskType,
};
use cauto::state::decision_log::{DecisionRecord, append_json_line, latest_route, timestamp_now};
use cauto::state::{build_report, prompt_sha256, sanitize_argv};
use proptest::prelude::*;
use tempfile::tempdir;

fn record(index: usize) -> DecisionRecord {
    let one = BoundedScore::new(1).unwrap();
    DecisionRecord {
        schema_version: 1,
        record_type: "decision".into(),
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
            "feedback": "right"
        }))
        .unwrap(),
    )
    .unwrap();
    let report = build_report(&path).unwrap();
    assert_eq!(report.total_decisions, 4);
    assert_eq!(report.average_confidence_basis_points, 8_000);
    assert_eq!(report.classifier_invocation_rate_basis_points, 5_000);
    assert_eq!(report.rules_most_often_raising_effort[0].0, "raise");
    assert_eq!(report.feedback_by_route["terra:medium"]["right"], 1);
}

#[test]
fn missing_history_returns_an_empty_report() {
    let root = tempdir().unwrap();
    let report = build_report(&root.path().join("missing.jsonl")).unwrap();
    assert_eq!(report.total_decisions, 0);
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

proptest! {
    #[test]
    fn arbitrary_raw_prompt_never_appears_in_record(prompt in "[A-Z]{24,64}") {
        let mut record = record(99);
        record.prompt_sha256 = prompt_sha256(OsString::from(&prompt).as_os_str());
        let json = serde_json::to_string(&record).unwrap();
        prop_assert!(!json.contains(&prompt));
    }
}
