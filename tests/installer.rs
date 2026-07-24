#![cfg(unix)]

use std::process::Command;

#[test]
fn shell_scripts_parse_with_bash() {
    for script in ["scripts/install.sh", "scripts/bench.sh"] {
        let status = Command::new("bash")
            .arg("-n")
            .arg(script)
            .current_dir(env!("CARGO_MANIFEST_DIR"))
            .status()
            .unwrap();
        assert!(status.success(), "{script} failed bash syntax validation");
    }
}

#[test]
fn installer_dry_run_uses_the_locked_validation_commands() {
    let output = Command::new("bash")
        .arg("scripts/install.sh")
        .arg("--dry-run")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "installer dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let plan = String::from_utf8(output.stdout).unwrap();

    assert!(plan.contains("+ cargo fmt --check\n"));
    assert!(plan.contains("+ cargo clippy --all-targets --all-features --locked -- -D warnings\n"));
    assert!(plan.contains("+ cargo test --all-targets --all-features --locked\n"));
    assert!(plan.contains("+ cargo install --path . --locked --force\n"));
}
