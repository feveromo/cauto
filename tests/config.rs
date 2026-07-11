use cauto::config::load;
use cauto::routing::{ClassifierMode, FastMode, ModelFamily, ReasoningLevel};
use proptest::prelude::*;
use tempfile::tempdir;

#[test]
fn project_overrides_user_rules_but_not_user_controls() {
    let root = tempdir().unwrap();
    let user = root.path().join("user.toml");
    let project = root.path().join("project.toml");
    std::fs::write(
        &user,
        r#"
version = 1
classifier = "never"
fast_mode = "fast"
default_model = "gpt-5.6-terra"
default_effort = "low"
ultra_requires_opt_in = true
allow_automatic_downgrade = false
strict_logging = true
catalog_cache_hours = 24
git_timeout_ms = 500
catalog_timeout_ms = 3000
classifier_timeout_seconds = 30
hysteresis_points = 4

[weights]
scope = 42

[[rules]]
id = "same"
phrases = ["user"]
family_floor = "terra"
"#,
    )
    .unwrap();
    std::fs::write(
        &project,
        r#"
version = 1

[[rules]]
id = "same"
phrases = ["project"]
family_floor = "sol"
effort_floor = "high"
"#,
    )
    .unwrap();
    let loaded = load(&user, Some(&project)).unwrap();
    assert_eq!(loaded.config.classifier, ClassifierMode::Never);
    assert_eq!(loaded.config.fast_mode, FastMode::Fast);
    assert_eq!(loaded.config.default_model, "gpt-5.6-terra");
    assert_eq!(loaded.config.default_effort, ReasoningLevel::Low);
    assert!(loaded.config.ultra_requires_opt_in);
    assert!(!loaded.config.allow_automatic_downgrade);
    assert!(loaded.config.strict_logging);
    assert_eq!(loaded.config.catalog_cache_hours.get(), 24);
    assert_eq!(loaded.config.git_timeout.get(), 500);
    assert_eq!(loaded.config.catalog_timeout.get(), 3_000);
    assert_eq!(loaded.config.classifier_timeout.get(), 30_000);
    assert_eq!(loaded.config.hysteresis_points, 4);
    assert_eq!(loaded.config.weights.scope, 42);
    assert_eq!(loaded.config.rules.len(), 1);
    assert_eq!(loaded.config.rules[0].phrases, ["project"]);
    assert_eq!(loaded.config.rules[0].family_floor, Some(ModelFamily::Sol));
    assert_eq!(
        loaded.config.rules[0].effort_floor,
        Some(ReasoningLevel::High)
    );
}

#[test]
fn project_policy_rejects_user_level_controls() {
    let root = tempdir().unwrap();
    let user = root.path().join("user.toml");
    let project = root.path().join("project.toml");
    std::fs::write(&user, "version = 1\nclassifier = \"never\"\n").unwrap();
    std::fs::write(
        &project,
        r#"
version = 1
classifier = "always"
ultra_requires_opt_in = false
default_effort = "ultra"
"#,
    )
    .unwrap();

    let error = load(&user, Some(&project)).unwrap_err().to_string();
    assert!(error.contains(project.to_str().unwrap()));
    assert!(error.contains("unknown field"));
    assert!(error.contains("classifier"));
}

#[test]
fn validation_names_file_and_toml_path() {
    let root = tempdir().unwrap();
    let user = root.path().join("bad.toml");
    std::fs::write(&user, "version = 2\n").unwrap();
    let error = load(&user, None).unwrap_err().to_string();
    assert!(error.contains(user.to_str().unwrap()));
    assert!(error.contains("version"));
    assert!(error.contains("expected 1"));
}

#[test]
fn contradictory_rule_is_rejected() {
    let root = tempdir().unwrap();
    let user = root.path().join("bad.toml");
    std::fs::write(
        &user,
        r#"
version = 1
[[rules]]
id = "bad"
phrases = ["x"]
family_floor = "sol"
family_ceiling = "luna"
"#,
    )
    .unwrap();
    assert!(load(&user, None).is_err());
}

#[test]
fn raw_prompt_logging_cannot_be_enabled() {
    let root = tempdir().unwrap();
    let user = root.path().join("bad.toml");
    std::fs::write(&user, "version = 1\nlog_raw_prompts = true\n").unwrap();
    assert!(load(&user, None).is_err());
}

proptest! {
    #[test]
    fn higher_layer_always_wins_for_scalar_values(
        lower in "[a-z]{1,16}",
        higher in "[a-z]{1,16}",
    ) {
        let low = cauto::config::RawConfig {
            default_model: Some(lower),
            ..cauto::config::RawConfig::default()
        };
        let high = cauto::config::RawConfig {
            default_model: Some(higher.clone()),
            ..cauto::config::RawConfig::default()
        };
        prop_assert_eq!(low.merge(high).default_model, Some(higher));
    }
}

#[test]
fn git_context_handles_absent_and_dirty_repositories() {
    let root = tempdir().unwrap();
    let timeout = cauto::config::schema::TimeoutMillis::new(500).unwrap();
    let absent = cauto::context::git::inspect(root.path(), false, timeout);
    assert_eq!(absent.state, cauto::context::GitState::NotRepository);

    std::process::Command::new("git")
        .args(["init", "-q"])
        .current_dir(root.path())
        .status()
        .unwrap();
    std::fs::write(root.path().join("dirty.txt"), "dirty").unwrap();
    let dirty = cauto::context::git::inspect(root.path(), true, timeout);
    assert_eq!(dirty.state, cauto::context::GitState::Dirty);
}

#[cfg(unix)]
#[test]
fn git_timeout_becomes_unknown_without_failing_route() {
    use std::os::unix::fs::PermissionsExt;

    let root = tempdir().unwrap();
    let program = root.path().join("slow-git");
    std::fs::write(&program, "#!/bin/sh\nsleep 5\n").unwrap();
    let mut permissions = program.metadata().unwrap().permissions();
    permissions.set_mode(0o755);
    std::fs::set_permissions(&program, permissions).unwrap();
    let context = cauto::context::git::inspect_with_program(
        root.path(),
        true,
        cauto::config::schema::TimeoutMillis::new(10).unwrap(),
        program.as_os_str(),
    );
    assert_eq!(context.state, cauto::context::GitState::Unknown);
}
