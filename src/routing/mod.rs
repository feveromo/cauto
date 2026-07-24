//! Pure deterministic task scoring and route selection.

/// Confidence calculation from evidence quality.
pub mod confidence;
/// Human-readable decision explanations.
pub mod explain;
mod features;
/// Compiled project and user policy rules.
pub mod rules;
/// Weighted complexity scoring.
pub mod score;
/// Family, effort, and Ultra selection.
pub mod select;
mod semantic;
mod task_budget;
/// Strong routing and launch-plan types.
pub mod types;

pub use confidence::{EvidenceQuality, confidence_for};
pub use features::FeatureAssessment;
pub use rules::{CompiledRules, RuleApplication};
pub use score::{Weights, normalized_score};
pub use select::SelectionConstraints;
pub use types::*;

/// Extracts bounded task evidence and applies semantic corrections for task shape and risk.
#[must_use]
pub fn extract_features(prompt: &str) -> FeatureAssessment {
    task_budget::apply(semantic::refine_features(features::extract_features(
        prompt,
    )))
}

/// Produces a complete deterministic route with post-score safety floors.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn route(
    task_type: TaskType,
    dimensions: DimensionScores,
    weights: Weights,
    constraints: SelectionConstraints,
    matched_rules: Vec<RuleMatch>,
    conflicts: Vec<Conflict>,
    evidence: EvidenceQuality,
    reasons: Vec<Reason>,
    escalation_signals: Vec<EscalationSignal>,
) -> RouteDecision {
    let explicit_family = constraints.explicit_family.clone();
    let explicit_effort = constraints.explicit_effort;
    let family_ceiling = constraints.family_ceiling.clone();
    let effort_ceiling = constraints.effort_ceiling;
    let risk_floor = semantic::risk_effort_floor(&task_type, dimensions);
    let mut decision = select::route(
        task_type,
        dimensions,
        weights,
        constraints,
        matched_rules,
        conflicts,
        evidence,
        reasons,
        escalation_signals,
    );
    if let Some(floor) = risk_floor {
        let mut escalated = false;
        if explicit_family.is_none()
            && decision.recommended_family.rank() < ModelFamily::Sol.rank()
            && family_ceiling
                .as_ref()
                .is_none_or(|ceiling| ModelFamily::Sol.rank() <= ceiling.rank())
        {
            decision.recommended_family = ModelFamily::Sol;
            escalated = true;
        }
        if explicit_effort.is_none()
            && decision.recommended_effort < floor.effort
            && effort_ceiling.is_none_or(|ceiling| floor.effort <= ceiling)
        {
            decision.recommended_effort = floor.effort;
            escalated = true;
        }
        if escalated {
            decision.reasons.push(Reason {
                label: floor.reason.into(),
                contribution: 1_000,
            });
            decision.escalation_signals.push(EscalationSignal {
                label: floor.reason.into(),
            });
        }
    }
    decision
}
