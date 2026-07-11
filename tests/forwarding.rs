use std::ffi::OsString;
use std::path::PathBuf;

use cauto::codex::args::inspect_forwarded;
use cauto::codex::launch::{InjectionPolicy, materialize_args};
use cauto::routing::{
    CapabilitySource, LaunchMode, LaunchPlan, ModelFamily, ReasoningLevel, RoutePreset,
};
use proptest::prelude::*;

fn plan(inherited_args: Vec<OsString>, prompt: Option<OsString>) -> LaunchPlan {
    LaunchPlan {
        codex_binary: PathBuf::from("/codex"),
        working_directory: PathBuf::from("/repo with spaces"),
        mode: LaunchMode::Interactive,
        preset: RoutePreset {
            model_id: "gpt-5.6-sol".into(),
            model_family: ModelFamily::Sol,
            display_level: ReasoningLevel::High,
            native_effort: Some("high".into()),
            collaboration_mode: None,
            service_tier: None,
            required_features: vec![],
            interactive_supported: true,
            exec_supported: true,
            source: CapabilitySource::Cache,
            fallback: None,
        },
        inherited_args,
        injected_args: vec![],
        prompt,
        downgrade: None,
    }
}

#[test]
fn explicit_native_model_effort_and_tier_are_detected() {
    let args = [
        "--model=gpt-5.6-sol",
        "-c",
        "model_reasoning_effort='high'",
        "--config=service_tier=\"priority\"",
        "-c",
        "unrelated=\"model=gpt-5.6-luna\"",
    ]
    .map(OsString::from);
    let overrides = inspect_forwarded(&args).unwrap();
    assert_eq!(overrides.model.as_deref(), Some("gpt-5.6-sol"));
    assert_eq!(overrides.effort, Some(ReasoningLevel::High));
    assert_eq!(overrides.service_tier.as_deref(), Some("priority"));
}

proptest! {
    #[test]
    fn arbitrary_forwarded_arguments_keep_boundaries(values in prop::collection::vec("[a-zA-Z0-9 _./=-]{0,32}", 0..32)) {
        let inherited: Vec<OsString> = values.iter().map(OsString::from).collect();
        let args = materialize_args(
            &plan(inherited.clone(), Some(OsString::from("prompt"))),
            InjectionPolicy { inject_model: true, inject_effort: true, inject_service_tier: false },
        );
        let start = args.len() - inherited.len() - 1;
        prop_assert_eq!(&args[start..start + inherited.len()], inherited.as_slice());
        prop_assert_eq!(args.last(), Some(&OsString::from("prompt")));
    }

    #[test]
    fn explicit_overrides_survive_arbitrary_unrelated_args(
        prefix in prop::collection::vec("[a-z]{1,12}", 0..16)
    ) {
        let mut args: Vec<OsString> = prefix.into_iter().map(OsString::from).collect();
        args.extend([
            OsString::from("--model"),
            OsString::from("gpt-5.6-terra"),
            OsString::from("-c"),
            OsString::from("model_reasoning_effort=\"medium\""),
        ]);
        let found = inspect_forwarded(&args).unwrap();
        prop_assert_eq!(found.model.as_deref(), Some("gpt-5.6-terra"));
        prop_assert_eq!(found.effort, Some(ReasoningLevel::Medium));
    }
}

#[cfg(unix)]
#[test]
fn non_utf8_forwarded_argument_is_preserved() {
    use std::os::unix::ffi::OsStringExt;

    let value = OsString::from_vec(vec![b'a', 0xff, b'b']);
    let args = materialize_args(&plan(vec![value.clone()], None), InjectionPolicy::default());
    assert_eq!(args.last(), Some(&value));
}
