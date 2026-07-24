use cauto::routing::{
    BoundedScore, DimensionScores, EvidenceQuality, FeatureAssessment, ModelFamily, ReasoningLevel,
    RouteDecision, SelectionConstraints, TaskType, Weights, extract_features, route,
};

fn route_for(prompt: &str) -> RouteDecision {
    let FeatureAssessment {
        task_type,
        dimensions,
        reasons,
        escalation_signals,
        ..
    } = extract_features(prompt);
    route(
        task_type,
        dimensions,
        Weights::default(),
        SelectionConstraints::default(),
        vec![],
        vec![],
        EvidenceQuality::default(),
        reasons,
        escalation_signals,
    )
}

#[test]
fn open_ended_repository_improvement_gets_real_headroom() {
    for prompt in [
        "look over my repo and make improvements to it",
        "make the router better and improve how it selects models",
        "review the repository and implement the highest-value fixes",
    ] {
        let decision = route_for(prompt);
        assert_eq!(decision.task_type, TaskType::Coding, "{prompt}");
        assert_eq!(decision.recommended_family, ModelFamily::Sol, "{prompt}");
        assert!(
            decision.recommended_effort >= ReasoningLevel::High,
            "{prompt}: {:?}",
            decision.dimensions
        );
    }
}

#[test]
fn simple_project_explanation_keeps_the_explicit_low_cost_budget() {
    let decision = route_for("what does this project do? explain it like i'm five");
    assert_eq!(decision.task_type, TaskType::Documentation);
    assert_eq!(decision.recommended_family, ModelFamily::Luna);
    assert_eq!(decision.recommended_effort, ReasoningLevel::Low);
}

#[test]
fn narrow_documentation_and_rename_work_stay_cheap() {
    for prompt in [
        "fix typo in README.md",
        "rename Foo to Bar in src/main.rs",
        "improve the README wording",
    ] {
        let decision = route_for(prompt);
        assert_eq!(decision.recommended_family, ModelFamily::Luna, "{prompt}");
        assert_eq!(decision.recommended_effort, ReasoningLevel::Low, "{prompt}");
    }
}

#[test]
fn documentation_words_do_not_hide_a_real_diagnosis() {
    let decision = route_for(
        "the README generator crashes intermittently; diagnose it and add a regression test",
    );
    assert_eq!(decision.task_type, TaskType::Diagnosis);
    assert_eq!(decision.recommended_family, ModelFamily::Sol);
    assert!(decision.recommended_effort >= ReasoningLevel::High);
}

#[test]
fn security_state_and_concurrency_work_have_high_reasoning_floors() {
    for prompt in [
        "rotate production credentials safely and verify that authentication still works",
        "migrate serialized state across the codebase with backward compatibility and tests",
        "investigate a race condition in the queue and prove the fix",
    ] {
        let decision = route_for(prompt);
        assert_eq!(decision.recommended_family, ModelFamily::Sol, "{prompt}");
        assert!(
            decision.recommended_effort >= ReasoningLevel::High,
            "{prompt}: score={}, dimensions={:?}",
            decision.normalized_score,
            decision.dimensions
        );
    }
}

#[test]
fn cross_platform_runtime_work_is_not_routed_as_an_everyday_edit() {
    let decision =
        route_for("make child process handling cross-platform for macOS and Linux and add tests");
    assert_eq!(decision.recommended_family, ModelFamily::Sol);
    assert!(decision.recommended_effort >= ReasoningLevel::High);
    assert!(decision.dimensions.runtime_dependence.get() >= 2);
    assert!(decision.dimensions.verification_burden.get() >= 3);
}

#[test]
fn precise_bounded_implementation_remains_terra_medium() {
    let decision =
        route_for("implement the known parser change in src/parser.rs with expected output");
    assert_eq!(decision.task_type, TaskType::Coding);
    assert_eq!(decision.recommended_family, ModelFamily::Terra);
    assert_eq!(decision.recommended_effort, ReasoningLevel::Medium);
}

#[test]
fn a_failing_unit_test_with_expected_output_is_not_over_escalated() {
    let decision = route_for("fix the failing unit test with expected output");
    assert_eq!(decision.task_type, TaskType::Diagnosis);
    assert_eq!(decision.recommended_family, ModelFamily::Terra);
    assert_eq!(decision.recommended_effort, ReasoningLevel::Medium);
}

#[test]
fn generic_provided_language_is_not_fake_acceptance_criteria() {
    let generic = extract_features("use the provided helper to update src/lib.rs");
    assert!(!generic.clear_completion);

    let failing = extract_features("the parser is failing; use the provided helper to fix it");
    assert_eq!(failing.task_type, TaskType::Diagnosis);
    assert!(!failing.clear_completion);
    assert_eq!(failing.dimensions.ambiguity.get(), 3);

    let specific =
        extract_features("use the provided test and expected output to update src/lib.rs");
    assert!(specific.clear_completion);
}

#[test]
fn critical_verification_floor_raises_both_family_and_effort() {
    let decision = route(
        TaskType::Review,
        DimensionScores {
            verification_burden: BoundedScore::new(4).unwrap(),
            ..DimensionScores::default()
        },
        Weights::default(),
        SelectionConstraints::default(),
        vec![],
        vec![],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    assert_eq!(decision.recommended_family, ModelFamily::Sol);
    assert_eq!(decision.recommended_effort, ReasoningLevel::High);
}

#[test]
fn explicit_family_and_effort_still_win_over_risk_floor() {
    let decision = route(
        TaskType::Diagnosis,
        DimensionScores {
            ambiguity: BoundedScore::new(3).unwrap(),
            cost_of_being_wrong: BoundedScore::new(3).unwrap(),
            verification_burden: BoundedScore::new(3).unwrap(),
            ..DimensionScores::default()
        },
        Weights::default(),
        SelectionConstraints {
            explicit_family: Some(ModelFamily::Terra),
            explicit_effort: Some(ReasoningLevel::Low),
            ..SelectionConstraints::default()
        },
        vec![],
        vec![],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    assert_eq!(decision.recommended_family, ModelFamily::Terra);
    assert_eq!(decision.recommended_effort, ReasoningLevel::Low);
}
