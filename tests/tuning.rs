mod common;

use std::ffi::OsString;

use cauto::routing::{
    BoundedScore, CapabilitySource, Conflict, DimensionScores, ModelFamily, ReasoningLevel,
    TaskType,
};
use cauto::state::decision_log::{
    DecisionRecord, append_json_line, prompt_sha256, repository_identifier, timestamp_now,
};
use cauto::state::{
    CalibrationStore, analyze_repository, build_report_with_calibrations, load_calibration,
    load_store, reset_repository, save_recommendation,
};
use predicates::prelude::*;
use tempfile::tempdir;

fn decision(id: &str, repository_id: &str, preview: bool) -> DecisionRecord {
    let one = BoundedScore::new(1).unwrap();
    DecisionRecord {
        schema_version: 1,
        record_type: "decision".into(),
        decision_mode: if preview { "preview" } else { "launched" }.into(),
        decision_id: id.into(),
        timestamp: timestamp_now(),
        cauto_version: "test".into(),
        codex_version: "test".into(),
        repository_identifier: repository_id.into(),
        repository_name: "repository".into(),
        git_branch: Some("main".into()),
        prompt_sha256: prompt_sha256(OsString::from("secret").as_os_str()),
        prompt_byte_length: 6,
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
        matched_rule_ids: vec![],
        raising_rule_ids: vec![],
        lowering_rule_ids: vec![],
        conflicts: Vec::<Conflict>::new(),
        selected_model: "gpt-5.6-terra".into(),
        selected_family: ModelFamily::Terra,
        selected_effort: ReasoningLevel::Medium,
        ultra_candidate: false,
        ultra_selected: false,
        classifier_ran: false,
        classifier_outcome: "skipped".into(),
        catalog_source: CapabilitySource::Cache,
        downgrade: None,
        sanitized_argv: vec![],
        feedback: None,
    }
}

fn append_feedback(path: &std::path::Path, id: &str, repository_id: &str, kind: &str) {
    append_json_line(
        path,
        &serde_json::to_vec(&serde_json::json!({
            "schema_version": 1,
            "record_type": "feedback",
            "decision_id": id,
            "repository_identifier": repository_id,
            "feedback": kind,
        }))
        .unwrap(),
    )
    .unwrap();
}

fn history_with_feedback(kinds: &[&str]) -> (tempfile::TempDir, std::path::PathBuf) {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");
    for (index, kind) in kinds.iter().enumerate() {
        let id = format!("id-{index}");
        append_json_line(
            &path,
            &serde_json::to_vec(&decision(&id, "repo", false)).unwrap(),
        )
        .unwrap();
        append_feedback(&path, &id, "repo", kind);
    }
    (root, path)
}

#[test]
fn fewer_than_three_and_mixed_feedback_are_ineligible() {
    let (_root, path) = history_with_feedback(&["underpowered", "underpowered"]);
    let analysis = analyze_repository(
        &path,
        &CalibrationStore::default(),
        Some(("repo", "repository")),
    )
    .unwrap();
    assert_eq!(analysis.repositories[0].status, "insufficient-feedback");
    assert_eq!(analysis.repositories[0].proposed_calibration, None);

    let (_root, path) = history_with_feedback(&["underpowered", "overkill", "right"]);
    let analysis = analyze_repository(
        &path,
        &CalibrationStore::default(),
        Some(("repo", "repository")),
    )
    .unwrap();
    assert_eq!(analysis.repositories[0].status, "mixed-signal");
    assert_eq!(analysis.repositories[0].proposed_calibration, None);
}

#[test]
fn directional_feedback_proposes_and_applies_bounded_offsets() {
    for (kind, expected) in [("underpowered", 5), ("overkill", -5)] {
        let (root, path) = history_with_feedback(&[kind, kind, kind]);
        let state = root.path().join("calibration.json");
        let mut store = CalibrationStore::default();
        let analysis = analyze_repository(&path, &store, Some(("repo", "repository"))).unwrap();
        let tuning = &analysis.repositories[0];
        assert!(tuning.eligible);
        assert_eq!(tuning.proposed_calibration, Some(expected));
        assert_eq!(
            save_recommendation(&state, &mut store, tuning).unwrap(),
            Some((None, expected))
        );
        assert_eq!(
            load_calibration(&state, "repo").unwrap().unwrap().points(),
            expected
        );
        let report = build_report_with_calibrations(&path, &state).unwrap();
        assert_eq!(
            report.feedback_by_repository[0].current_calibration,
            Some(expected)
        );
        assert_eq!(
            reset_repository(&state, &mut store, "repo").unwrap(),
            Some(expected)
        );
        assert!(load_calibration(&state, "repo").unwrap().is_none());
    }
}

#[test]
fn previews_and_diagnostic_failures_do_not_tune() {
    let root = tempdir().unwrap();
    let path = root.path().join("decisions.jsonl");
    append_json_line(
        &path,
        &serde_json::to_vec(&decision("preview", "repo", true)).unwrap(),
    )
    .unwrap();
    for _ in 0..3 {
        append_feedback(&path, "preview", "repo", "underpowered");
    }
    append_json_line(
        &path,
        &serde_json::to_vec(&decision("real", "repo", false)).unwrap(),
    )
    .unwrap();
    for _ in 0..3 {
        append_feedback(&path, "real", "repo", "failed-for-other-reason");
    }
    let analysis = analyze_repository(
        &path,
        &CalibrationStore::default(),
        Some(("repo", "repository")),
    )
    .unwrap();
    let tuning = &analysis.repositories[0];
    assert_eq!(tuning.previews_excluded, 3);
    assert_eq!(tuning.eligible_feedback_count, 0);
    assert_eq!(tuning.feedback.failed_for_other_reason, 3);
    assert!(!tuning.eligible);
}

#[test]
fn malformed_and_missing_calibration_fail_safely() {
    let root = tempdir().unwrap();
    let missing = root.path().join("missing.json");
    assert!(load_calibration(&missing, "repo").unwrap().is_none());
    let malformed = root.path().join("malformed.json");
    std::fs::write(&malformed, b"not json").unwrap();
    assert!(load_calibration(&malformed, "repo").is_err());
}

#[cfg(unix)]
#[test]
fn calibration_state_uses_restrictive_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let (root, decisions) =
        history_with_feedback(&["underpowered", "underpowered", "underpowered"]);
    let path = root.path().join("private/calibration.json");
    let mut store = CalibrationStore::default();
    let analysis = analyze_repository(&decisions, &store, Some(("repo", "repository"))).unwrap();
    save_recommendation(&path, &mut store, &analysis.repositories[0]).unwrap();
    assert_eq!(path.metadata().unwrap().permissions().mode() & 0o777, 0o600);
    assert_eq!(
        path.parent()
            .unwrap()
            .metadata()
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o700
    );
}

#[test]
fn tune_cli_is_read_only_then_apply_and_reset_are_repository_scoped() {
    let home = tempdir().unwrap();
    let repository = home.path().join("repository");
    std::fs::create_dir(&repository).unwrap();
    let repository_id = repository_identifier(&repository);
    let decisions = home.path().join("state/cauto/decisions.jsonl");
    for index in 0..3 {
        let id = format!("id-{index}");
        append_json_line(
            &decisions,
            &serde_json::to_vec(&decision(&id, &repository_id, false)).unwrap(),
        )
        .unwrap();
        append_feedback(&decisions, &id, &repository_id, "underpowered");
    }
    let calibration = home.path().join("state/cauto/calibration.json");

    common::cauto_command(home.path())
        .args(["--repo", repository.to_str().unwrap(), "tune"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Recommendation: +5 points"));
    assert!(!calibration.exists());

    common::cauto_command(home.path())
        .args(["--repo", repository.to_str().unwrap(), "tune", "--apply"])
        .assert()
        .success()
        .stdout(predicate::str::contains("none -> +5 points"));
    assert_eq!(
        load_store(&calibration).unwrap().repositories[&repository_id].score_offset,
        5
    );

    let codex = common::fake_codex(home.path());
    let calibrated_output = common::cauto_command(home.path())
        .args([
            "--repo",
            repository.to_str().unwrap(),
            "--codex-bin",
            codex.to_str().unwrap(),
            "--no-classifier",
            "--dry-run",
            "--json",
            "investigate a bounded bug",
        ])
        .output()
        .unwrap();
    assert!(calibrated_output.status.success());
    let calibrated: serde_json::Value = serde_json::from_slice(&calibrated_output.stdout).unwrap();
    assert_eq!(
        calibrated["decision"]["calibration"]["configured_offset"],
        5
    );
    assert_eq!(calibrated["decision"]["calibration"]["applied_offset"], 5);
    common::cauto_command(home.path())
        .args([
            "--repo",
            repository.to_str().unwrap(),
            "--codex-bin",
            codex.to_str().unwrap(),
            "--no-classifier",
            "--dry-run",
            "investigate a bounded bug",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains(
            "Calibration: configured +5, applied +5",
        ));

    common::cauto_command(home.path())
        .args(["--repo", repository.to_str().unwrap(), "tune", "--reset"])
        .assert()
        .success()
        .stdout(predicate::str::contains("+5 -> none"));
    assert!(
        !load_store(&calibration)
            .unwrap()
            .repositories
            .contains_key(&repository_id)
    );

    let baseline_output = common::cauto_command(home.path())
        .args([
            "--repo",
            repository.to_str().unwrap(),
            "--codex-bin",
            codex.to_str().unwrap(),
            "--no-classifier",
            "--dry-run",
            "--json",
            "investigate a bounded bug",
        ])
        .output()
        .unwrap();
    assert!(baseline_output.status.success());
    let baseline: serde_json::Value = serde_json::from_slice(&baseline_output.stdout).unwrap();
    assert!(baseline["decision"]["calibration"].is_null());
    assert_eq!(
        calibrated["decision"]["score"].as_u64().unwrap(),
        baseline["decision"]["score"].as_u64().unwrap() + 5
    );
}

#[test]
fn malformed_calibration_does_not_block_cli_routing() {
    let home = tempdir().unwrap();
    let repository = home.path().join("repository");
    std::fs::create_dir(&repository).unwrap();
    let calibration = home.path().join("state/cauto/calibration.json");
    std::fs::create_dir_all(calibration.parent().unwrap()).unwrap();
    std::fs::write(&calibration, b"malformed").unwrap();
    let codex = common::fake_codex(home.path());
    common::cauto_command(home.path())
        .args([
            "--repo",
            repository.to_str().unwrap(),
            "--codex-bin",
            codex.to_str().unwrap(),
            "--no-classifier",
            "--dry-run",
            "--json",
            "investigate a bounded bug",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"calibration\": null"));
}
