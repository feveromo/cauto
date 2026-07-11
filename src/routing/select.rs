use super::{
    Conflict, DimensionScores, ModelFamily, ReasoningLevel, RouteDecision, RuleMatch, TaskType,
    Weights,
    confidence::EvidenceQuality,
    confidence_for,
    score::{dimension_reasons, normalized_score},
};

/// Floors, ceilings, and explicit choices applied after base routing.
#[derive(Clone, Debug, Default)]
pub struct SelectionConstraints {
    /// Minimum permitted model family.
    pub family_floor: Option<ModelFamily>,
    /// Maximum permitted model family.
    pub family_ceiling: Option<ModelFamily>,
    /// Minimum permitted reasoning level.
    pub effort_floor: Option<ReasoningLevel>,
    /// Maximum permitted reasoning level.
    pub effort_ceiling: Option<ReasoningLevel>,
    /// Explicit family override, which takes precedence over policy bounds.
    pub explicit_family: Option<ModelFamily>,
    /// Explicit effort override, which takes precedence over policy bounds.
    pub explicit_effort: Option<ReasoningLevel>,
    /// Whether the user or applicable instructions authorize Ultra delegation.
    pub ultra_authorized: bool,
    /// Whether the task actually contains independent parallel tracks.
    pub meaningful_parallel_tracks: bool,
    /// Score points around a boundary in which a prior effort is retained.
    pub hysteresis_points: u8,
    /// Most recent bounded model family for this repository, when available.
    pub prior_family: Option<ModelFamily>,
    /// Most recent bounded route for this repository, when available.
    pub prior_effort: Option<ReasoningLevel>,
}

/// Selects the lowest family compatible with score and high-risk dimensions.
#[must_use]
pub fn family_for(dimensions: DimensionScores, score: u8) -> ModelFamily {
    if score <= 20
        && dimensions.ambiguity.get() <= 1
        && dimensions.cost_of_being_wrong.get() <= 1
        && dimensions.runtime_dependence.get() <= 1
        && dimensions.architectural_depth.get() <= 1
    {
        ModelFamily::Luna
    } else if score <= 45
        && dimensions.ambiguity.get() <= 2
        && dimensions.cost_of_being_wrong.get() <= 2
        && dimensions.runtime_dependence.get() <= 2
        && dimensions.architectural_depth.get() <= 2
    {
        ModelFamily::Terra
    } else {
        ModelFamily::Sol
    }
}

/// Retains an adjacent prior family inside a score boundary band.
#[must_use]
pub fn family_with_hysteresis(
    dimensions: DimensionScores,
    score: u8,
    prior: Option<ModelFamily>,
    margin: u8,
) -> ModelFamily {
    let selected = family_for(dimensions, score);
    let Some(prior) = prior else {
        return selected;
    };
    let luna_safe = dimensions.ambiguity.get() <= 1
        && dimensions.cost_of_being_wrong.get() <= 1
        && dimensions.runtime_dependence.get() <= 1
        && dimensions.architectural_depth.get() <= 1;
    let terra_safe = dimensions.ambiguity.get() <= 2
        && dimensions.cost_of_being_wrong.get() <= 2
        && dimensions.runtime_dependence.get() <= 2
        && dimensions.architectural_depth.get() <= 2;
    let retain = match (&prior, &selected) {
        (ModelFamily::Luna, ModelFamily::Terra) => score.abs_diff(20) <= margin && luna_safe,
        (ModelFamily::Terra, ModelFamily::Luna) => score.abs_diff(20) <= margin,
        (ModelFamily::Terra, ModelFamily::Sol) => score.abs_diff(45) <= margin && terra_safe,
        (ModelFamily::Sol, ModelFamily::Terra) => score.abs_diff(45) <= margin,
        _ => false,
    };
    if retain { prior } else { selected }
}

/// Maps a normalized complexity score to the initial sequential effort.
#[must_use]
pub const fn effort_for(score: u8) -> ReasoningLevel {
    match score {
        0..=20 => ReasoningLevel::Low,
        21..=45 => ReasoningLevel::Medium,
        46..=68 => ReasoningLevel::High,
        69..=84 => ReasoningLevel::ExtraHigh,
        _ => ReasoningLevel::Max,
    }
}

/// Keeps a prior effort inside a configurable threshold band to avoid route flapping.
#[must_use]
pub fn effort_with_hysteresis(
    score: u8,
    prior: Option<ReasoningLevel>,
    margin: u8,
) -> ReasoningLevel {
    let selected = effort_for(score);
    let Some(prior) = prior else {
        return selected;
    };
    let boundary = match (prior, selected) {
        (ReasoningLevel::Low, ReasoningLevel::Medium) => Some(20),
        (ReasoningLevel::Medium, ReasoningLevel::Low) => Some(20),
        (ReasoningLevel::Medium, ReasoningLevel::High) => Some(45),
        (ReasoningLevel::High, ReasoningLevel::Medium) => Some(45),
        (ReasoningLevel::High, ReasoningLevel::ExtraHigh) => Some(68),
        (ReasoningLevel::ExtraHigh, ReasoningLevel::High) => Some(68),
        (ReasoningLevel::ExtraHigh, ReasoningLevel::Max) => Some(84),
        (ReasoningLevel::Max, ReasoningLevel::ExtraHigh) => Some(84),
        _ => None,
    };
    if boundary.is_some_and(|point| score.abs_diff(point) <= margin) {
        prior
    } else {
        selected
    }
}

fn apply_family_constraints(
    mut family: ModelFamily,
    constraints: &SelectionConstraints,
    conflicts: &mut Vec<Conflict>,
) -> ModelFamily {
    if let (Some(floor), Some(ceiling)) = (&constraints.family_floor, &constraints.family_ceiling)
        && floor.rank() > ceiling.rank()
    {
        conflicts.push(Conflict {
            kind: "family-floor-ceiling".into(),
            message: format!(
                "family floor {floor} conflicts with ceiling {ceiling}; safety floor wins"
            ),
        });
    }
    if let Some(floor) = &constraints.family_floor
        && family.rank() < floor.rank()
    {
        family = floor.clone();
    }
    if let Some(ceiling) = &constraints.family_ceiling
        && family.rank() > ceiling.rank()
        && constraints
            .family_floor
            .as_ref()
            .is_none_or(|floor| floor.rank() <= ceiling.rank())
    {
        family = ceiling.clone();
    }
    constraints.explicit_family.clone().unwrap_or(family)
}

fn apply_effort_constraints(
    mut effort: ReasoningLevel,
    constraints: &SelectionConstraints,
    conflicts: &mut Vec<Conflict>,
) -> ReasoningLevel {
    if let (Some(floor), Some(ceiling)) = (constraints.effort_floor, constraints.effort_ceiling)
        && floor > ceiling
    {
        conflicts.push(Conflict {
            kind: "effort-floor-ceiling".into(),
            message: format!(
                "effort floor {} conflicts with ceiling {}; safety floor wins",
                floor.display_name(),
                ceiling.display_name()
            ),
        });
    }
    if let Some(floor) = constraints.effort_floor
        && effort < floor
    {
        effort = floor;
    }
    if let Some(ceiling) = constraints.effort_ceiling
        && effort > ceiling
        && constraints
            .effort_floor
            .is_none_or(|floor| floor <= ceiling)
    {
        effort = ceiling;
    }
    constraints.explicit_effort.unwrap_or(effort)
}

/// Produces a complete deterministic route from dimensions and validated constraints.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn route(
    task_type: TaskType,
    dimensions: DimensionScores,
    weights: Weights,
    constraints: SelectionConstraints,
    matched_rules: Vec<RuleMatch>,
    mut conflicts: Vec<Conflict>,
    mut evidence: EvidenceQuality,
    mut reasons: Vec<super::Reason>,
    escalation_signals: Vec<super::EscalationSignal>,
) -> RouteDecision {
    let score = normalized_score(dimensions, weights);
    let mut family = apply_family_constraints(
        family_with_hysteresis(
            dimensions,
            score,
            constraints.prior_family.clone(),
            constraints.hysteresis_points,
        ),
        &constraints,
        &mut conflicts,
    );
    let mut effort = effort_with_hysteresis(
        score,
        constraints.prior_effort,
        constraints.hysteresis_points,
    );
    let ultra_candidate = score >= 69
        && dimensions.parallelizability.get() >= 3
        && constraints.meaningful_parallel_tracks;
    if ultra_candidate && constraints.explicit_effort.is_none() {
        effort = if constraints.ultra_authorized {
            ReasoningLevel::Ultra
        } else {
            ReasoningLevel::Max
        };
        family = ModelFamily::Sol;
    }
    effort = apply_effort_constraints(effort, &constraints, &mut conflicts);
    if effort == ReasoningLevel::Ultra && !constraints.ultra_authorized {
        conflicts.push(Conflict {
            kind: "ultra-authorization".into(),
            message: "Ultra requires explicit subagent authorization; selected sequential Max"
                .into(),
        });
        effort = ReasoningLevel::Max;
    }
    evidence.conflict_count = conflicts.len() as u16;
    reasons.extend(dimension_reasons(dimensions, weights));
    RouteDecision {
        task_type,
        dimensions,
        normalized_score: score,
        confidence: confidence_for(evidence),
        matched_rules,
        conflicts,
        recommended_family: family,
        recommended_effort: effort,
        ultra_candidate,
        ultra_selected: effort == ReasoningLevel::Ultra,
        reasons,
        escalation_signals,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::BoundedScore;

    #[test]
    fn ultra_requires_authorization() {
        let four = BoundedScore::new(4).unwrap();
        let dimensions = DimensionScores {
            scope: four,
            ambiguity: four,
            cost_of_being_wrong: four,
            runtime_dependence: four,
            architectural_depth: four,
            verification_burden: four,
            parallelizability: four,
        };
        let decision = route(
            TaskType::Architecture,
            dimensions,
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
        assert!(decision.ultra_candidate);
        assert!(!decision.ultra_selected);
        assert_eq!(decision.recommended_effort, ReasoningLevel::Max);
    }

    #[test]
    fn explicit_override_wins_over_floor() {
        let decision = route(
            TaskType::Coding,
            DimensionScores::default(),
            Weights::default(),
            SelectionConstraints {
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
        assert_eq!(decision.recommended_family, ModelFamily::Luna);
        assert_eq!(decision.recommended_effort, ReasoningLevel::Low);
    }
}
