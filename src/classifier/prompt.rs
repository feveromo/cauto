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
        "Classify this Codex task only. Return JSON matching the supplied schema. \
Do not recommend a model, construct commands, authorize delegation, or infer file contents. \
Judge the work requested, not the prompt's word count, tone, or formatting. A terse failure can \
still be difficult; a long pasted handoff is not difficult merely because it is long.\n\n\
Use this 0 through 4 rubric:\n\
- scope: 0 none, 1 one local operation, 2 bounded component, 3 multiple surfaces, 4 broad program.\n\
- ambiguity: 0 fully mechanical, 1 clear implementation, 2 some discovery, 3 unknown root cause, 4 open-ended research.\n\
- cost_of_being_wrong: 0 disposable, 1 easy local correction, 2 meaningful rework, 3 live/user/system impact, 4 destructive/security/production impact.\n\
- runtime_dependence: 0 static files only, 1 ordinary tests, 2 bounded process behavior, 3 live app/service/browser/device state, 4 fragile distributed or account-affecting state.\n\
- architectural_depth: 0 text/data only, 1 local code, 2 component boundary, 3 subsystem/protocol boundary, 4 cross-system redesign or reverse engineering.\n\
- verification_burden: 0 no proof needed, 1 focused check, 2 normal test suite, 3 live or multi-surface proof, 4 repeated experiments or failure recovery.\n\
- parallelizability: 0 sequential, 1 incidental split, 2 two related tracks, 3 three independent tracks, 4 many independent workstreams.\n\n\
Use task_type documentation, mechanical, coding, diagnosis, architecture, research, or review. \
Treat an app that will not launch, a service that cannot connect, or an unexplained runtime error \
as diagnosis with substantial ambiguity/runtime/verification evidence.\n\n{}",
        serde_json::to_string(&input)?
    ))
}
