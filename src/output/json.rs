use serde::Serialize;

use crate::routing::{Downgrade, LaunchMode, RouteDecision, RoutePreset};

#[derive(Serialize)]
struct JsonDecision<'a> {
    model: &'a str,
    family: &'a crate::routing::ModelFamily,
    effort: crate::routing::ReasoningLevel,
    score: u8,
    confidence_basis_points: u16,
    ultra_candidate: bool,
    ultra_selected: bool,
    catalog_source: &'a crate::routing::CapabilitySource,
    downgrade: Option<&'a Downgrade>,
}

#[derive(Serialize)]
struct JsonLaunch<'a> {
    mode: LaunchMode,
    working_directory: &'a str,
    prompt_redacted: bool,
}

#[derive(Serialize)]
struct JsonClassifier<'a> {
    ran: bool,
    outcome: &'a str,
}

#[derive(Serialize)]
struct JsonOutput<'a> {
    schema_version: u32,
    decision: JsonDecision<'a>,
    dimensions: crate::routing::DimensionScores,
    matches: &'a [crate::routing::RuleMatch],
    conflicts: &'a [crate::routing::Conflict],
    classifier: JsonClassifier<'a>,
    launch: JsonLaunch<'a>,
}

pub fn render(
    decision: &RouteDecision,
    preset: &RoutePreset,
    downgrade: Option<&Downgrade>,
    mode: LaunchMode,
    working_directory: &str,
    classifier_ran: bool,
    classifier_outcome: &str,
) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(&JsonOutput {
        schema_version: 1,
        decision: JsonDecision {
            model: &preset.model_id,
            family: &preset.model_family,
            effort: preset.display_level,
            score: decision.normalized_score,
            confidence_basis_points: decision.confidence.basis_points(),
            ultra_candidate: decision.ultra_candidate,
            ultra_selected: decision.ultra_selected,
            catalog_source: &preset.source,
            downgrade,
        },
        dimensions: decision.dimensions,
        matches: &decision.matched_rules,
        conflicts: &decision.conflicts,
        classifier: JsonClassifier {
            ran: classifier_ran,
            outcome: classifier_outcome,
        },
        launch: JsonLaunch {
            mode,
            working_directory,
            prompt_redacted: true,
        },
    })
}
