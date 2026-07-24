mod common;

use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn explain_shows_the_risk_aware_route_and_dimension_trace() {
    let home = tempdir().unwrap();
    let codex = common::fake_codex(home.path());

    common::cauto_command(home.path())
        .args([
            "--codex-bin",
            codex.to_str().unwrap(),
            "explain",
            "--prompt",
            "look over my repo and make improvements to it",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Selected: gpt-5.6-sol / High"))
        .stdout(predicate::str::contains("Task: coding"))
        .stdout(predicate::str::contains("Decision trace:"))
        .stdout(predicate::str::contains("Why: scope, ambiguity"))
        .stdout(predicate::str::contains("dimensions: scope=3, ambiguity=3"));
}

#[test]
fn explain_preserves_the_simple_project_explanation_budget() {
    let home = tempdir().unwrap();
    let codex = common::fake_codex(home.path());

    common::cauto_command(home.path())
        .args([
            "--codex-bin",
            codex.to_str().unwrap(),
            "explain",
            "--prompt",
            "what does this project do? explain it like i'm five",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Selected: gpt-5.6-luna / Low"))
        .stdout(predicate::str::contains("Task: documentation"));
}
