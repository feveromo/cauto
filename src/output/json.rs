use serde::Serialize;

use crate::routing::{
    CalibrationEffect, Downgrade, LaunchMode, RouteDecision, RoutePreset, RouteSource,
};

#[derive(Serialize)]
struct JsonDecision<'a> {
    task_type: &'a crate::routing::TaskType,
    model: &'a str,
    family: &'a crate::routing::ModelFamily,
    effort: crate::routing::ReasoningLevel,
    score: u8,
    calibration: Option<&'a CalibrationEffect>,
    confidence_basis_points: u16,
    ultra_candidate: bool,
    ultra_selected: bool,
    catalog_source: &'a crate::routing::CapabilitySource,
    downgrade: Option<&'a Downgrade>,
    route_source: RouteSource,
}

#[derive(Serialize)]
struct JsonLaunch<'a> {
    mode: LaunchMode,
    working_directory: &'a str,
    prompt_redacted: bool,
}

#[derive(Serialize)]
struct JsonRouting {
    elapsed_micros: u64,
}

#[derive(Serialize)]
struct JsonOutput<'a> {
    schema_version: u32,
    decision: JsonDecision<'a>,
    dimensions: crate::routing::DimensionScores,
    matches: &'a [crate::routing::RuleMatch],
    conflicts: &'a [crate::routing::Conflict],
    reasons: &'a [crate::routing::Reason],
    escalation_signals: &'a [crate::routing::EscalationSignal],
    routing: JsonRouting,
    launch: JsonLaunch<'a>,
}

pub fn render(
    decision: &RouteDecision,
    preset: &RoutePreset,
    downgrade: Option<&Downgrade>,
    mode: LaunchMode,
    working_directory: &str,
    route_source: RouteSource,
    routing_elapsed_micros: u64,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&JsonOutput {
        schema_version: 2,
        decision: JsonDecision {
            task_type: &decision.task_type,
            model: &preset.model_id,
            family: &preset.model_family,
            effort: preset.display_level,
            score: decision.normalized_score,
            calibration: decision.calibration.as_ref(),
            confidence_basis_points: decision.confidence.basis_points(),
            ultra_candidate: decision.ultra_candidate,
            ultra_selected: decision.ultra_selected,
            catalog_source: &preset.source,
            downgrade,
            route_source,
        },
        dimensions: decision.dimensions,
        matches: &decision.matched_rules,
        conflicts: &decision.conflicts,
        reasons: &decision.reasons,
        escalation_signals: &decision.escalation_signals,
        routing: JsonRouting {
            elapsed_micros: routing_elapsed_micros,
        },
        launch: JsonLaunch {
            mode,
            working_directory,
            prompt_redacted: true,
        },
    })
}
