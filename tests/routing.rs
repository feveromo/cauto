use cauto::routing::select::effort_for;
use cauto::routing::select::effort_with_hysteresis;
use cauto::routing::select::family_with_hysteresis;
use cauto::routing::{
    BoundedScore, Conflict, DimensionScores, EvidenceQuality, ModelFamily, ReasoningLevel,
    ScoreCalibration, SelectionConstraints, TaskType, Weights, normalized_score, route,
};
use proptest::prelude::*;

fn dimensions(values: [u8; 7]) -> DimensionScores {
    DimensionScores {
        scope: BoundedScore::new(values[0]).unwrap(),
        ambiguity: BoundedScore::new(values[1]).unwrap(),
        cost_of_being_wrong: BoundedScore::new(values[2]).unwrap(),
        runtime_dependence: BoundedScore::new(values[3]).unwrap(),
        architectural_depth: BoundedScore::new(values[4]).unwrap(),
        verification_burden: BoundedScore::new(values[5]).unwrap(),
        parallelizability: BoundedScore::new(values[6]).unwrap(),
    }
}

proptest! {
    #[test]
    fn arbitrary_dimensions_always_score_in_range(values in prop::array::uniform7(0_u8..=4)) {
        let score = normalized_score(dimensions(values), Weights::default());
        prop_assert!(score <= 100);
    }

    #[test]
    fn selection_is_deterministic(values in prop::array::uniform7(0_u8..=4)) {
        let dimensions = dimensions(values);
        let first = route(
            TaskType::Coding, dimensions, Weights::default(),
            SelectionConstraints::default(), vec![], vec![], EvidenceQuality::default(), vec![], vec![],
        );
        let second = route(
            TaskType::Coding, dimensions, Weights::default(),
            SelectionConstraints::default(), vec![], vec![], EvidenceQuality::default(), vec![], vec![],
        );
        prop_assert_eq!(first, second);
    }

    #[test]
    fn raising_one_risk_dimension_never_lowers_effort(
        values in prop::array::uniform7(0_u8..=4),
        index in 0_usize..6,
    ) {
        let before = dimensions(values);
        let mut raised = values;
        raised[index] = raised[index].saturating_add(1).min(4);
        let after = dimensions(raised);
        prop_assert!(effort_for(normalized_score(after, Weights::default()))
            >= effort_for(normalized_score(before, Weights::default())));
    }
}

#[test]
fn family_floor_is_never_violated() {
    let decision = route(
        TaskType::Documentation,
        dimensions([0, 0, 0, 0, 0, 0, 0]),
        Weights::default(),
        SelectionConstraints {
            family_floor: Some(ModelFamily::Sol),
            ..SelectionConstraints::default()
        },
        vec![],
        vec![],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    assert_eq!(decision.recommended_family, ModelFamily::Sol);
}

#[test]
fn unsupported_ultra_cannot_be_selected_without_authorization() {
    let decision = route(
        TaskType::Architecture,
        dimensions([4, 4, 4, 4, 4, 4, 4]),
        Weights::default(),
        SelectionConstraints {
            meaningful_parallel_tracks: true,
            ..SelectionConstraints::default()
        },
        vec![],
        vec![],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    assert_eq!(decision.recommended_effort, ReasoningLevel::Max);
    assert!(!decision.ultra_selected);
}

#[test]
fn hysteresis_keeps_prior_effort_inside_boundary_band() {
    assert_eq!(
        effort_with_hysteresis(46, Some(ReasoningLevel::Medium), 2),
        ReasoningLevel::Medium
    );
    assert_eq!(
        effort_with_hysteresis(50, Some(ReasoningLevel::Medium), 2),
        ReasoningLevel::High
    );
}

#[test]
fn family_hysteresis_respects_boundary_and_risk_guards() {
    let safe = dimensions([1, 1, 1, 1, 1, 1, 0]);
    assert_eq!(
        family_with_hysteresis(safe, 21, Some(ModelFamily::Luna), 2),
        ModelFamily::Luna
    );
    let architectural = dimensions([1, 1, 1, 1, 3, 1, 0]);
    assert_eq!(
        family_with_hysteresis(architectural, 44, Some(ModelFamily::Terra), 2),
        ModelFamily::Sol
    );
}

#[test]
fn route_applies_repository_hysteresis() {
    let decision = route(
        TaskType::Coding,
        dimensions([2, 2, 2, 1, 2, 2, 0]),
        Weights::default(),
        SelectionConstraints {
            prior_effort: Some(ReasoningLevel::Medium),
            hysteresis_points: 2,
            ..SelectionConstraints::default()
        },
        vec![],
        vec![],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    assert_eq!(decision.normalized_score, 46);
    assert_eq!(decision.recommended_effort, ReasoningLevel::Medium);
}

#[test]
fn rule_conflicts_survive_selection_and_reduce_confidence() {
    let constraints = SelectionConstraints {
        family_floor: Some(ModelFamily::Sol),
        family_ceiling: Some(ModelFamily::Luna),
        ..SelectionConstraints::default()
    };
    let baseline = route(
        TaskType::Coding,
        DimensionScores::default(),
        Weights::default(),
        constraints.clone(),
        vec![],
        vec![],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    let decision = route(
        TaskType::Coding,
        DimensionScores::default(),
        Weights::default(),
        constraints,
        vec![],
        vec![Conflict {
            kind: "matched-family-bounds".into(),
            message: "conflict produced while applying matched rules".into(),
        }],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );

    assert!(
        decision
            .conflicts
            .iter()
            .any(|conflict| conflict.kind == "matched-family-bounds")
    );
    assert!(
        decision
            .conflicts
            .iter()
            .any(|conflict| conflict.kind == "family-floor-ceiling")
    );
    assert!(decision.confidence < baseline.confidence);
}

#[test]
fn calibration_is_bounded_and_composes_before_hysteresis() {
    assert!(ScoreCalibration::new(11).is_err());
    let decision = route(
        TaskType::Coding,
        dimensions([2, 2, 2, 1, 2, 2, 0]),
        Weights::default(),
        SelectionConstraints {
            calibration: ScoreCalibration::new(-5).unwrap(),
            prior_effort: Some(ReasoningLevel::High),
            hysteresis_points: 2,
            ..SelectionConstraints::default()
        },
        vec![],
        vec![],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    let calibration = decision.calibration.unwrap();
    assert_eq!(calibration.base_score, 46);
    assert_eq!(calibration.calibrated_score, 41);
    assert_eq!(decision.recommended_effort, ReasoningLevel::Medium);
}

#[test]
fn upward_calibration_does_not_escalate_docs_or_mechanical_tasks() {
    for task_type in [TaskType::Documentation, TaskType::Mechanical] {
        let decision = route(
            task_type,
            dimensions([2, 2, 2, 1, 2, 2, 0]),
            Weights::default(),
            SelectionConstraints {
                calibration: ScoreCalibration::new(10).unwrap(),
                ..SelectionConstraints::default()
            },
            vec![],
            vec![],
            EvidenceQuality::default(),
            vec![],
            vec![],
        );
        let calibration = decision.calibration.unwrap();
        assert_eq!(calibration.applied_offset, 0);
        assert_eq!(calibration.base_score, calibration.calibrated_score);
    }
}

#[test]
fn calibration_cannot_force_max_or_override_explicit_choices_and_floors() {
    let near_max = route(
        TaskType::Coding,
        dimensions([4, 3, 3, 3, 3, 3, 0]),
        Weights::default(),
        SelectionConstraints {
            calibration: ScoreCalibration::new(10).unwrap(),
            ..SelectionConstraints::default()
        },
        vec![],
        vec![],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    assert_eq!(near_max.normalized_score, 84);
    assert_eq!(near_max.recommended_effort, ReasoningLevel::ExtraHigh);

    let floor_only = route(
        TaskType::Coding,
        DimensionScores::default(),
        Weights::default(),
        SelectionConstraints {
            calibration: ScoreCalibration::new(-10).unwrap(),
            family_floor: Some(ModelFamily::Sol),
            effort_floor: Some(ReasoningLevel::High),
            ..SelectionConstraints::default()
        },
        vec![],
        vec![],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    assert_eq!(floor_only.recommended_family, ModelFamily::Sol);
    assert_eq!(floor_only.recommended_effort, ReasoningLevel::High);

    let constrained = route(
        TaskType::Coding,
        DimensionScores::default(),
        Weights::default(),
        SelectionConstraints {
            calibration: ScoreCalibration::new(-10).unwrap(),
            family_floor: Some(ModelFamily::Sol),
            effort_floor: Some(ReasoningLevel::High),
            explicit_family: Some(ModelFamily::Luna),
            explicit_effort: Some(ReasoningLevel::Low),
            ..SelectionConstraints::default()
        },
        vec![],
        vec![],
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    assert_eq!(constrained.recommended_family, ModelFamily::Luna);
    assert_eq!(constrained.recommended_effort, ReasoningLevel::Low);
}
