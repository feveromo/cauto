use serde::Serialize;

use crate::context::{ContextSnapshot, GitState};
use crate::routing::RouteDecision;

#[derive(Serialize)]
struct ClassifierInput<'a> {
    task: &'a str,
    repository: &'a str,
    top_level_subsystems: &'a [String],
    git_state: &'a GitState,
    matched_project_rules: Vec<&'a str>,
    deterministic_dimensions: crate::routing::DimensionScores,
    deterministic_conflicts: Vec<&'a str>,
}

pub fn build_classifier_prompt(
    task: &str,
    context: &ContextSnapshot,
    decision: &RouteDecision,
) -> Result<String, serde_json::Error> {
    let input = ClassifierInput {
        task,
        repository: &context.repository.name,
        top_level_subsystems: &context.repository.top_level_names,
        git_state: &context.git.state,
        matched_project_rules: decision
            .matched_rules
            .iter()
            .map(|matched| matched.rule_id.as_str())
            .collect(),
        deterministic_dimensions: decision.dimensions,
        deterministic_conflicts: decision
            .conflicts
            .iter()
            .map(|conflict| conflict.message.as_str())
            .collect(),
    };
    Ok(format!(
        "Classify this coding task only. Return JSON matching the supplied schema. \
Score each dimension from 0 through 4 using the provided definitions. \
Do not recommend a model, construct commands, authorize delegation, or infer file contents.\n\n{}",
        serde_json::to_string(&input)?
    ))
}
