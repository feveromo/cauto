use std::path::{Path, PathBuf};
use std::time::Duration;

use cauto::classifier::runner;
use cauto::classifier::{ClassifierAssessment, blend_dimensions, should_run};
use cauto::codex::binary::CodexInstallation;
use cauto::routing::{
    BoundedScore, ClassifierMode, Confidence, DimensionScores, TaskType, Weights, normalized_score,
};
use tempfile::tempdir;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn installation(binary: PathBuf) -> CodexInstallation {
    CodexInstallation {
        canonical_binary: binary.clone(),
        binary,
        fingerprint: "test".into(),
        codex_home: PathBuf::from("/tmp/codex-home"),
        codex_home_hash: "home".into(),
        profile: None,
    }
}

#[cfg(unix)]
fn script(directory: &Path, body: &str) -> PathBuf {
    let path = directory.join("codex");
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    let mut permissions = path.metadata().unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&path, permissions).unwrap();
    path
}

#[test]
fn classifier_output_is_strictly_bounded() {
    let valid = br#"{
      "task_type":"coding",
      "scope":2,
      "ambiguity":3,
      "cost_of_being_wrong":1,
      "runtime_dependence":2,
      "architectural_depth":2,
      "verification_burden":3,
      "parallelizability":1,
      "confidence":0.75,
      "reasons":["bounded"],
      "escalation_signals":[],
      "model":"ignored-future-field"
    }"#;
    let parsed = ClassifierAssessment::parse(valid).unwrap();
    assert_eq!(parsed.scope.get(), 2);
    assert_eq!(parsed.task_type, TaskType::Coding);
    assert_eq!(parsed.confidence.basis_points(), 7_500);

    let invalid = valid.to_vec();
    let text = String::from_utf8(invalid)
        .unwrap()
        .replace("\"scope\":2", "\"scope\":5");
    assert!(ClassifierAssessment::parse(text.as_bytes()).is_err());
    assert!(ClassifierAssessment::parse(b"not json").is_err());
}

#[test]
fn classifier_can_add_semantic_risk_but_cannot_erase_deterministic_evidence() {
    let assessment = ClassifierAssessment::parse(
        br#"{
          "task_type":"diagnosis",
          "scope":2,
          "ambiguity":3,
          "cost_of_being_wrong":3,
          "runtime_dependence":3,
          "architectural_depth":1,
          "verification_burden":3,
          "parallelizability":0,
          "confidence":0.9,
          "reasons":["live failure"],
          "escalation_signals":[]
        }"#,
    )
    .unwrap();
    let merged = blend_dimensions(DimensionScores::default(), &assessment);
    assert!(normalized_score(merged, Weights::default()) >= 46);
    assert_eq!(merged.runtime_dependence.get(), 3);

    let four = BoundedScore::new(4).unwrap();
    let deterministic = DimensionScores {
        scope: four,
        ambiguity: four,
        cost_of_being_wrong: four,
        runtime_dependence: four,
        architectural_depth: four,
        verification_burden: four,
        parallelizability: four,
    };
    let merged = blend_dimensions(deterministic, &assessment);
    assert_eq!(merged, deterministic);
}

#[test]
fn classifier_gate_respects_explicit_and_offline_routes() {
    let low = Confidence::from_basis_points(5_000).unwrap();
    assert!(should_run(
        ClassifierMode::Auto,
        low,
        7_200,
        false,
        0,
        true,
        false,
        false,
        true
    ));
    assert!(!should_run(
        ClassifierMode::Always,
        low,
        7_200,
        true,
        0,
        true,
        true,
        false,
        true
    ));
    assert!(!should_run(
        ClassifierMode::Always,
        low,
        7_200,
        true,
        0,
        true,
        false,
        true,
        true
    ));
}

#[cfg(unix)]
#[test]
fn classifier_runner_accepts_valid_native_output() {
    let root = tempdir().unwrap();
    let binary = script(
        root.path(),
        r#"
out=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--output-last-message" ]; then
    shift
    out="$1"
  fi
  shift
done
printf '%s\n' '{"task_type":"coding","scope":1,"ambiguity":1,"cost_of_being_wrong":1,"runtime_dependence":0,"architectural_depth":1,"verification_burden":1,"parallelizability":0,"confidence":0.9,"reasons":["clear"],"escalation_signals":[]}' > "$out"
"#,
    );
    let run = runner::run(
        &installation(binary),
        "gpt-5.6-luna",
        "classify this",
        Duration::from_secs(2),
    )
    .unwrap();
    assert_eq!(run.category, "success");
}

#[cfg(unix)]
#[test]
fn classifier_runner_times_out_and_reaps_group() {
    let root = tempdir().unwrap();
    let binary = script(root.path(), "sleep 5");
    let error = runner::run(
        &installation(binary),
        "gpt-5.6-luna",
        "classify this",
        Duration::from_millis(20),
    )
    .unwrap_err();
    assert!(matches!(error, runner::ClassifierError::Timeout));
}

#[cfg(unix)]
#[test]
fn classifier_runner_rejects_malformed_json() {
    let root = tempdir().unwrap();
    let binary = script(
        root.path(),
        r#"
out=""
while [ "$#" -gt 0 ]; do
  if [ "$1" = "--output-last-message" ]; then shift; out="$1"; fi
  shift
done
printf 'malformed' > "$out"
"#,
    );
    let error = runner::run(
        &installation(binary),
        "gpt-5.6-luna",
        "classify",
        Duration::from_secs(2),
    )
    .unwrap_err();
    assert!(matches!(error, runner::ClassifierError::InvalidOutput(_)));
}
