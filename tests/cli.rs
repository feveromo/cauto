mod common;

use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn help_and_version_are_fast_paths() {
    common::cauto_command(tempdir().unwrap().path())
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("Repository-aware"));
    common::cauto_command(tempdir().unwrap().path())
        .arg("--version")
        .assert()
        .success()
        .stdout(predicate::str::contains(env!("CARGO_PKG_VERSION")));
}

#[test]
fn empty_noninteractive_prompt_is_rejected() {
    common::cauto_command(tempdir().unwrap().path())
        .arg("--dry-run")
        .assert()
        .code(2)
        .stderr(predicate::str::contains("no task prompt"));
}

#[test]
fn multiline_prompt_is_redacted_from_json() {
    let home = tempdir().unwrap();
    let codex = common::fake_codex(home.path());
    let prompt = "first line\nsecond secret line";
    let output = common::cauto_command(home.path())
        .args([
            "--codex-bin",
            codex.to_str().unwrap(),
            "--no-classifier",
            "--dry-run",
            "--json",
            "--prompt",
            prompt,
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(!stdout.contains(prompt));
    let value: serde_json::Value = serde_json::from_str(&stdout).unwrap();
    assert_eq!(value["launch"]["prompt_redacted"], true);
}

#[test]
fn prompt_file_and_stdin_are_supported() {
    let home = tempdir().unwrap();
    let codex = common::fake_codex(home.path());
    let prompt_path = home.path().join("task with spaces.txt");
    std::fs::write(&prompt_path, "fix a typo").unwrap();
    common::cauto_command(home.path())
        .args([
            "--codex-bin",
            codex.to_str().unwrap(),
            "--no-classifier",
            "--dry-run",
            "--prompt-file",
            prompt_path.to_str().unwrap(),
        ])
        .assert()
        .success();

    let mut command = common::cauto_command(home.path());
    command.args([
        "--codex-bin",
        codex.to_str().unwrap(),
        "--no-classifier",
        "--dry-run",
        "--stdin",
    ]);
    command.write_stdin("add a focused test\nwith two lines");
    command.assert().success();
}

#[test]
fn forwarding_requires_delimiter_and_preserves_repeated_images() {
    let home = tempdir().unwrap();
    let codex = common::fake_codex(home.path());
    let output = common::cauto_command(home.path())
        .args([
            "--codex-bin",
            codex.to_str().unwrap(),
            "--no-classifier",
            "--dry-run",
            "--print-command",
            "--prompt",
            "inspect images",
            "--",
            "--image",
            "one image.png",
            "--image",
            "two.png",
            "--search",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("--image 'one image.png' --image two.png --search"));
}

#[test]
fn completions_are_generated() {
    common::cauto_command(tempdir().unwrap().path())
        .args(["completions", "bash"])
        .assert()
        .success()
        .stdout(predicate::str::contains("_cauto"));
}

#[test]
fn cauto_model_and_effort_overrides_win() {
    let home = tempdir().unwrap();
    let codex = common::fake_codex(home.path());
    let output = common::cauto_command(home.path())
        .args([
            "--codex-bin",
            codex.to_str().unwrap(),
            "--model",
            "gpt-5.6-terra",
            "--effort",
            "medium",
            "--dry-run",
            "--json",
            "investigate this bug",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["decision"]["model"], "gpt-5.6-terra");
    assert_eq!(value["decision"]["effort"], "medium");
}

#[test]
fn forwarded_native_overrides_are_not_duplicated() {
    let home = tempdir().unwrap();
    let codex = common::fake_codex(home.path());
    let output = common::cauto_command(home.path())
        .args([
            "--codex-bin",
            codex.to_str().unwrap(),
            "--no-classifier",
            "--dry-run",
            "--print-command",
            "--prompt",
            "investigate this bug",
            "--",
            "--model",
            "gpt-5.6-sol",
            "-c",
            "model_reasoning_effort=\"high\"",
            "-c",
            "service_tier=\"priority\"",
        ])
        .output()
        .unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert_eq!(stdout.matches("--model").count(), 1);
    assert_eq!(stdout.matches("model_reasoning_effort").count(), 1);
    assert_eq!(stdout.matches("service_tier").count(), 1);
}

#[test]
fn explicit_fast_uses_catalog_tier() {
    let home = tempdir().unwrap();
    let codex = common::fake_codex(home.path());
    common::cauto_command(home.path())
        .args([
            "--codex-bin",
            codex.to_str().unwrap(),
            "--no-classifier",
            "--fast",
            "--dry-run",
            "--print-command",
            "fix a typo",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("service_tier=\"priority\""));
}

#[test]
fn explicit_ultra_does_not_claim_ineligibility() {
    let home = tempdir().unwrap();
    let codex = common::fake_codex(home.path());
    common::cauto_command(home.path())
        .args([
            "--codex-bin",
            codex.to_str().unwrap(),
            "--no-classifier",
            "--effort",
            "ultra",
            "--allow-ultra",
            "--dry-run",
            "rename a local variable",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Ultra: selected"))
        .stdout(predicate::str::contains("Ultra: not eligible").not());
}

#[test]
fn resumed_sessions_are_not_automatically_rerouted() {
    let home = tempdir().unwrap();
    common::cauto_command(home.path())
        .args(["--prompt", "task", "--", "resume"])
        .assert()
        .code(2)
        .stderr(predicate::str::contains(
            "does not reroute resumed sessions",
        ));
}
