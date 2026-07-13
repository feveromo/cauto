use std::collections::HashMap;
use std::ffi::OsString;
use std::process::ExitCode;
use std::str::FromStr;

use serde_json::{Value, json};

use crate::app_server::{MessageInterceptor, ProcessConfig};
use crate::cli::{AgentArgs, GlobalArgs, RouteArgs};
use crate::codex::args::{ExplicitNativeOverrides, inspect_forwarded};
use crate::error::AppError;
use crate::routing::{LaunchMode, ModelFamily, ReasoningLevel};
use crate::state::{
    FeedbackKind, FeedbackSource, analyze_repository, append_feedback_for_decision, load_store,
    save_recommendation,
};

use super::decision::{DecisionLogInput, write as write_decision};
use super::route::{OutcomeBias, ResolvedRoute, SessionRouteContext, resolve_route};
use super::{load_context_and_config, resolve_installation};

#[derive(Clone, Debug, Default)]
struct ExplicitRoute {
    model: Option<String>,
    effort: Option<ReasoningLevel>,
}

#[derive(Clone, Debug, Default)]
struct ThreadState {
    family: Option<ModelFamily>,
    effort: Option<ReasoningLevel>,
    model: Option<String>,
    decision_id: Option<String>,
    repository_id: Option<String>,
    repository_name: Option<String>,
    consecutive_failed_turns: u8,
    next_explicit_route: Option<ExplicitRoute>,
    feedback_recorded_for_decision: bool,
}

struct PendingTurn {
    thread_id: String,
    resolved: ResolvedRoute,
}

struct AgentRouter {
    global: GlobalArgs,
    base_args: RouteArgs,
    native: ExplicitNativeOverrides,
    threads: HashMap<String, ThreadState>,
    pending: HashMap<String, PendingTurn>,
    pending_resumes: HashMap<String, String>,
}

fn request_key(value: &Value) -> Option<String> {
    value
        .get("id")
        .and_then(|id| serde_json::to_string(id).ok())
}

fn warning(thread_id: Option<&str>, message: impl Into<String>) -> Value {
    json!({
        "method": "warning",
        "params": {
            "threadId": thread_id,
            "message": message.into(),
        }
    })
}

fn prompt_text(params: &Value) -> Option<String> {
    let mut fragments = Vec::new();
    for input in params.get("input")?.as_array()? {
        if input.get("type").and_then(Value::as_str) == Some("text")
            && let Some(text) = input.get("text").and_then(Value::as_str)
        {
            fragments.push(text);
        }
    }
    (!fragments.is_empty()).then(|| fragments.join("\n"))
}

fn contains_any(text: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| text.contains(phrase))
}

fn implicit_bias(prompt: &str) -> Option<OutcomeBias> {
    let text = prompt.to_ascii_lowercase();
    if contains_any(
        &text,
        &[
            "that's wrong",
            "that is wrong",
            "you got that wrong",
            "still broken",
            "still failing",
            "still doesn't work",
            "still doesnt work",
            "didn't fix",
            "did not fix",
            "you missed",
            "you didn't",
            "you did not",
            "try again",
            "do it properly",
            "not what i asked",
            "that's not what",
            "that is not what",
        ],
    ) || text.trim_start().starts_with("no,")
    {
        Some(OutcomeBias::Underpowered)
    } else if contains_any(
        &text,
        &[
            "that was overkill",
            "this is overkill",
            "way too much",
            "keep it simple",
            "don't overthink",
            "do not overthink",
            "use a simpler approach",
        ],
    ) {
        Some(OutcomeBias::Overkill)
    } else {
        None
    }
}

fn feedback_for_route_change(
    previous_family: Option<&ModelFamily>,
    previous_effort: Option<ReasoningLevel>,
    next_model: Option<&str>,
    next_effort: Option<ReasoningLevel>,
) -> Option<FeedbackKind> {
    let next_family = next_model.map(ModelFamily::from_model_id);
    let raised = previous_family
        .zip(next_family.as_ref())
        .is_some_and(|(previous, next)| next.rank() > previous.rank())
        || previous_effort
            .zip(next_effort)
            .is_some_and(|(previous, next)| next > previous);
    let lowered = previous_family
        .zip(next_family.as_ref())
        .is_some_and(|(previous, next)| next.rank() < previous.rank())
        || previous_effort
            .zip(next_effort)
            .is_some_and(|(previous, next)| next < previous);
    match (raised, lowered) {
        (true, false) => Some(FeedbackKind::Underpowered),
        (false, true) => Some(FeedbackKind::Overkill),
        _ => None,
    }
}

fn parse_explicit_route(params: &Value) -> ExplicitRoute {
    let settings = params
        .get("collaborationMode")
        .and_then(|mode| mode.get("settings"));
    let model = settings
        .and_then(|settings| settings.get("model"))
        .or_else(|| params.get("model"))
        .and_then(Value::as_str)
        .map(str::to_owned);
    let effort_value =
        if settings.is_some_and(|settings| settings.get("reasoning_effort").is_some()) {
            settings.and_then(|settings| settings.get("reasoning_effort"))
        } else {
            params.get("effort")
        };
    ExplicitRoute {
        model,
        effort: effort_value
            .and_then(Value::as_str)
            .and_then(|value| ReasoningLevel::from_str(value).ok()),
    }
}

fn rewrite_turn_params(
    params: &mut serde_json::Map<String, Value>,
    model: &str,
    effort: Option<&str>,
    service_tier: Option<&str>,
) {
    params.insert("model".into(), Value::String(model.to_owned()));
    if let Some(effort) = effort {
        params.insert("effort".into(), Value::String(effort.to_owned()));
    } else {
        params.remove("effort");
    }
    if let Some(settings) = params
        .get_mut("collaborationMode")
        .and_then(|mode| mode.get_mut("settings"))
        .and_then(Value::as_object_mut)
    {
        settings.insert("model".into(), Value::String(model.to_owned()));
        settings.insert(
            "reasoning_effort".into(),
            effort.map_or(Value::Null, |value| Value::String(value.to_owned())),
        );
    }
    if let Some(service_tier) = service_tier {
        params.insert("serviceTier".into(), Value::String(service_tier.to_owned()));
    }
}

impl AgentRouter {
    fn new(global: GlobalArgs, base_args: RouteArgs, native: ExplicitNativeOverrides) -> Self {
        Self {
            global,
            base_args,
            native,
            threads: HashMap::new(),
            pending: HashMap::new(),
            pending_resumes: HashMap::new(),
        }
    }

    fn record_feedback(
        state: &mut ThreadState,
        paths: &crate::paths::CautoPaths,
        kind: FeedbackKind,
        source: FeedbackSource,
    ) -> Result<Option<String>, AppError> {
        if state.feedback_recorded_for_decision {
            return Ok(None);
        }
        let (Some(decision_id), Some(repository_id), Some(repository_name)) = (
            state.decision_id.as_deref(),
            state.repository_id.as_deref(),
            state.repository_name.as_deref(),
        ) else {
            return Ok(None);
        };
        append_feedback_for_decision(&paths.decisions(), decision_id, repository_id, kind, source)?;
        state.feedback_recorded_for_decision = true;

        let mut store = load_store(&paths.calibration())?;
        let analysis = analyze_repository(
            &paths.decisions(),
            &store,
            Some((repository_id, repository_name)),
        )?;
        let Some(tuning) = analysis.repositories.first() else {
            return Ok(None);
        };
        let change = save_recommendation(&paths.calibration(), &mut store, tuning)?;
        Ok(change.map(|(before, after)| {
            format!(
                "automatic repository calibration changed from {} to {after:+} points",
                before.map_or_else(|| "none".into(), |value| format!("{value:+}"))
            )
        }))
    }

    fn note_explicit_route(
        &mut self,
        thread_id: &str,
        params: &Value,
    ) -> Result<Vec<Value>, AppError> {
        let mut explicit = parse_explicit_route(params);
        if self.base_args.model.is_some()
            || self.base_args.family.is_some()
            || self.native.model.is_some()
        {
            explicit.model = None;
        }
        if self.base_args.effort.is_some() || self.native.effort_raw.is_some() {
            explicit.effort = None;
        }
        if explicit.model.is_none() && explicit.effort.is_none() {
            return Ok(vec![warning(
                Some(thread_id),
                "cauto kept the command-line route pin; restart without the model/effort override to change it in-session",
            )]);
        }
        let state = self.threads.entry(thread_id.to_owned()).or_default();
        let feedback = feedback_for_route_change(
            state.family.as_ref(),
            state.effort,
            explicit.model.as_deref(),
            explicit.effort,
        );
        state.next_explicit_route = Some(explicit);
        let Some(feedback) = feedback else {
            return Ok(Vec::new());
        };
        let calibration = crate::paths::CautoPaths::discover().and_then(|paths| {
            Self::record_feedback(state, &paths, feedback, FeedbackSource::ExplicitRouteChange)
        });
        let direction = match feedback {
            FeedbackKind::Underpowered => "higher",
            FeedbackKind::Overkill => "lower",
            FeedbackKind::Right | FeedbackKind::FailedForOtherReason => "different",
        };
        let messages = match calibration {
            Ok(calibration) => {
                let mut messages = vec![warning(
                    Some(thread_id),
                    format!("cauto learned from the explicit {direction} route selection"),
                )];
                if let Some(calibration) = calibration {
                    messages.push(warning(Some(thread_id), format!("cauto: {calibration}")));
                }
                messages
            }
            Err(error) => vec![warning(
                Some(thread_id),
                format!(
                    "cauto will honor the explicit route, but automatic feedback could not be persisted: {error}"
                ),
            )],
        };
        Ok(messages)
    }

    fn route_turn(
        &mut self,
        request: &mut Value,
        thread_id: &str,
        prompt: String,
    ) -> Result<Vec<Value>, AppError> {
        let state = self.threads.entry(thread_id.to_owned()).or_default();
        let prompt_bias = implicit_bias(&prompt);
        let repeated_failure_bias = prompt_bias.is_none() && state.consecutive_failed_turns >= 2;
        let mut bias = prompt_bias;
        if repeated_failure_bias {
            bias = Some(OutcomeBias::Underpowered);
        }

        let mut turn_args = self.base_args.clone();
        turn_args.task = Some(OsString::from(&prompt));
        turn_args.prompt = None;
        turn_args.prompt_file = None;
        turn_args.stdin = false;
        turn_args.dry_run = false;
        turn_args.print_command = false;
        let explicit = state.next_explicit_route.take().or_else(|| {
            let incoming = parse_explicit_route(request.get("params").unwrap_or(&Value::Null));
            let model_changed = incoming
                .model
                .as_ref()
                .is_some_and(|model| state.model.as_ref().is_some_and(|current| current != model));
            let effort_changed = incoming
                .effort
                .zip(state.effort)
                .is_some_and(|(incoming, current)| incoming != current);
            (state.decision_id.is_some() && (model_changed || effort_changed)).then_some(incoming)
        });
        if self.base_args.model.is_none()
            && self.base_args.family.is_none()
            && self.native.model.is_none()
            && let Some(model) = explicit.as_ref().and_then(|value| value.model.clone())
        {
            turn_args.model = Some(model);
        }
        if self.base_args.effort.is_none()
            && self.native.effort_raw.is_none()
            && let Some(effort) = explicit.as_ref().and_then(|value| value.effort)
        {
            turn_args.effort = Some(effort.native_name().into());
        }

        let session = SessionRouteContext {
            prior_family: state.family.clone(),
            prior_effort: state.effort,
            outcome_bias: bias,
        };
        let mut turn_global = self.global.clone();
        if turn_global.repo.is_none()
            && let Some(cwd) = request
                .get("params")
                .and_then(|params| params.get("cwd"))
                .and_then(Value::as_str)
        {
            turn_global.repo = Some(cwd.into());
        }
        let resolved = resolve_route(
            &turn_global,
            &turn_args,
            LaunchMode::Interactive,
            false,
            Some(&session),
        )?;

        let mut calibration_message = None;
        let mut feedback_error = None;
        if let Some(bias) = prompt_bias
            && state.decision_id.is_some()
        {
            let kind = match bias {
                OutcomeBias::Underpowered => FeedbackKind::Underpowered,
                OutcomeBias::Overkill => FeedbackKind::Overkill,
            };
            let source = FeedbackSource::ImplicitCorrection;
            match Self::record_feedback(state, &resolved.paths, kind, source) {
                Ok(message) => calibration_message = message,
                Err(error) => feedback_error = Some(error),
            }
        }

        let params = request
            .get_mut("params")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| AppError::AppServer("turn/start params were not an object".into()))?;
        rewrite_turn_params(
            params,
            &resolved.plan.preset.model_id,
            resolved.plan.preset.native_effort.as_deref(),
            resolved.plan.preset.service_tier.as_deref(),
        );
        let previous = state
            .family
            .as_ref()
            .zip(state.effort)
            .map(|(family, effort)| format!("{family}/{}", effort.native_name()));
        let selected = format!(
            "{}/{}",
            resolved.plan.preset.model_family,
            resolved.plan.preset.display_level.native_name()
        );
        let route_note = previous.map_or_else(
            || format!("cauto routed this thread to {selected}"),
            |previous| {
                if previous == selected {
                    format!("cauto kept the thread on {selected}")
                } else {
                    format!("cauto changed the thread route: {previous} -> {selected}")
                }
            },
        );
        let key = request_key(request)
            .ok_or_else(|| AppError::AppServer("turn/start request had no id".into()))?;
        self.pending.insert(
            key,
            PendingTurn {
                thread_id: thread_id.to_owned(),
                resolved,
            },
        );
        let mut messages = vec![warning(Some(thread_id), route_note)];
        if repeated_failure_bias {
            messages.push(warning(
                Some(thread_id),
                "cauto escalated this turn after two failed turns; infrastructure failures do not affect persistent calibration",
            ));
        }
        if let Some(calibration_message) = calibration_message {
            messages.push(warning(
                Some(thread_id),
                format!("cauto: {calibration_message}"),
            ));
        }
        if let Some(error) = feedback_error {
            messages.push(warning(
                Some(thread_id),
                format!(
                    "cauto applied the adaptive route, but automatic feedback could not be persisted: {error}"
                ),
            ));
        }
        Ok(messages)
    }

    fn commit_turn(&mut self, key: &str, response: &Value) -> Result<Vec<Value>, AppError> {
        let Some(pending) = self.pending.remove(key) else {
            return Ok(Vec::new());
        };
        if response.get("error").is_some() {
            return Ok(vec![warning(
                Some(&pending.thread_id),
                "cauto route was not launched because App Server rejected turn/start",
            )]);
        }
        let ResolvedRoute {
            context,
            loaded,
            paths,
            catalog,
            prompt,
            decision,
            plan,
            policy,
            classifier_ran,
            classifier_outcome,
            ..
        } = pending.resolved;
        let decision_result = write_decision(DecisionLogInput {
            paths: &paths,
            context: &context,
            catalog: &catalog,
            prompt: &prompt,
            decision: &decision,
            plan: &plan,
            policy,
            classifier_ran,
            classifier_outcome: &classifier_outcome,
            decision_mode: "agent",
            strict: loaded.config.strict_logging,
            quiet: self.global.quiet,
        });
        let (decision_id, logging_warning) = match decision_result {
            Ok(decision_id) => (decision_id, None),
            Err(error) => (
                None,
                Some(warning(
                    Some(&pending.thread_id),
                    format!(
                        "cauto launched the routed turn, but strict decision logging failed; automatic feedback is disabled for this decision: {error}"
                    ),
                )),
            ),
        };
        let state = self.threads.entry(pending.thread_id.clone()).or_default();
        state.family = Some(plan.preset.model_family.clone());
        state.effort = Some(plan.preset.display_level);
        state.model = Some(plan.preset.model_id);
        state.decision_id = decision_id;
        state.repository_id = Some(crate::state::repository_identifier(
            &context.repository.root,
        ));
        state.repository_name = Some(context.repository.name);
        state.feedback_recorded_for_decision = false;
        Ok(logging_warning.into_iter().collect())
    }

    fn note_completion(&mut self, message: &Value) {
        let Some(params) = message.get("params") else {
            return;
        };
        let Some(thread_id) = params.get("threadId").and_then(Value::as_str) else {
            return;
        };
        let status = params
            .get("turn")
            .and_then(|turn| turn.get("status"))
            .and_then(Value::as_str);
        let state = self.threads.entry(thread_id.to_owned()).or_default();
        match status {
            Some("failed") => {
                state.consecutive_failed_turns = state.consecutive_failed_turns.saturating_add(1);
            }
            Some("completed") => state.consecutive_failed_turns = 0,
            Some("interrupted") | None | Some(_) => {}
        }
    }

    fn note_resume_response(&mut self, thread_id: &str, message: &Value) {
        if message.get("error").is_some() {
            return;
        }
        let Some(result) = message.get("result") else {
            return;
        };
        let model = result.get("model").and_then(Value::as_str);
        let effort = result
            .get("reasoningEffort")
            .and_then(Value::as_str)
            .and_then(|value| ReasoningLevel::from_str(value).ok());
        let state = self.threads.entry(thread_id.to_owned()).or_default();
        if let Some(model) = model {
            state.family = Some(ModelFamily::from_model_id(model));
            state.model = Some(model.to_owned());
        }
        if effort.is_some() {
            state.effort = effort;
        }
    }
}

impl MessageInterceptor for AgentRouter {
    fn client_message(&mut self, message: &mut Value) -> Result<Vec<Value>, AppError> {
        let method = message.get("method").and_then(Value::as_str);
        if method == Some("thread/resume")
            && let (Some(key), Some(thread_id)) = (
                request_key(message),
                message
                    .get("params")
                    .and_then(|params| params.get("threadId"))
                    .and_then(Value::as_str),
            )
        {
            self.pending_resumes.insert(key, thread_id.to_owned());
            return Ok(Vec::new());
        }
        if method == Some("thread/settings/update") {
            let thread_id = message
                .get("params")
                .and_then(|params| params.get("threadId"))
                .and_then(Value::as_str)
                .map(str::to_owned);
            if let Some(thread_id) = thread_id {
                return self.note_explicit_route(
                    &thread_id,
                    message.get("params").unwrap_or(&Value::Null),
                );
            }
        }
        if method != Some("turn/start") {
            return Ok(Vec::new());
        }
        let Some(params) = message.get("params") else {
            return Ok(Vec::new());
        };
        let Some(thread_id) = params
            .get("threadId")
            .and_then(Value::as_str)
            .map(str::to_owned)
        else {
            return Ok(Vec::new());
        };
        let Some(prompt) = prompt_text(params) else {
            return Ok(vec![warning(
                Some(&thread_id),
                "cauto left this turn unchanged because it contained no text input",
            )]);
        };
        match self.route_turn(message, &thread_id, prompt) {
            Ok(messages) => Ok(messages),
            Err(error) => Ok(vec![warning(
                Some(&thread_id),
                format!("cauto routing failed; native route preserved: {error}"),
            )]),
        }
    }

    fn server_message(&mut self, message: &Value) -> Result<Vec<Value>, AppError> {
        if message.get("method").and_then(Value::as_str) == Some("turn/completed") {
            self.note_completion(message);
            return Ok(Vec::new());
        }
        if let Some(key) = request_key(message) {
            if let Some(thread_id) = self.pending_resumes.remove(&key) {
                self.note_resume_response(&thread_id, message);
                return Ok(Vec::new());
            }
            if self.pending.contains_key(&key) {
                return self.commit_turn(&key, message);
            }
        }
        Ok(Vec::new())
    }
}

fn validate_args(args: &AgentArgs) -> Result<(), AppError> {
    if args
        .route
        .forwarded
        .first()
        .and_then(|value| value.to_str())
        == Some("resume")
    {
        return Err(AppError::InvalidArguments(
            "use `cauto agent --resume THREAD_ID` instead of forwarding resume".into(),
        ));
    }
    Ok(())
}

fn tui_args(args: &AgentArgs, prompt: Option<OsString>) -> Vec<OsString> {
    let mut values = Vec::new();
    if let Some(thread_id) = &args.resume {
        values.push(OsString::from("resume"));
        values.push(OsString::from(thread_id));
    }
    values.extend(args.route.forwarded.iter().cloned());
    if let Some(prompt) = prompt {
        values.push(prompt);
    }
    values
}

pub(super) fn run(global: &GlobalArgs, args: AgentArgs) -> Result<ExitCode, AppError> {
    validate_args(&args)?;
    if global.json {
        return Err(AppError::InvalidArguments(
            "--json is not supported by the native TUI agent mode".into(),
        ));
    }
    let route: RouteArgs = args.route.clone().into();
    let initial_prompt = super::prompt::acquire(&route, LaunchMode::Interactive)?.original;
    let native = inspect_forwarded(&route.forwarded)?;
    let (context, _, _) = load_context_and_config(global, Some(&route))?;
    let installation = resolve_installation(global, &native)?;
    let mut base_args = route;
    base_args.task = None;
    base_args.prompt = None;
    base_args.prompt_file = None;
    base_args.stdin = false;
    let mut router = AgentRouter::new(global.clone(), base_args, native);
    let config = ProcessConfig {
        binary: installation.binary,
        working_directory: context.repository.root,
        profile: installation.profile,
        tui_args: tui_args(&args, initial_prompt),
        verbose: global.verbose,
    };
    let (exit, capabilities) = crate::app_server::run(config, &mut router)?;
    if global.verbose {
        eprintln!(
            "cauto: App Server negotiated {} models, {} collaboration modes, and {} experimental features (namespace tools: {}, image generation: {}, web search: {})",
            capabilities.model_count,
            capabilities.collaboration_mode_count,
            capabilities.experimental_feature_count,
            capabilities.namespace_tools,
            capabilities.image_generation,
            capabilities.web_search,
        );
    }
    Ok(exit)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{BoundedScore, CapabilitySource, Conflict, DimensionScores, TaskType};
    use crate::state::decision_log::{append_json_line, timestamp_now};
    use crate::state::{DecisionRecord, build_report, prompt_sha256};
    use tempfile::tempdir;

    fn decision_record(id: &str) -> DecisionRecord {
        let one = BoundedScore::new(1).unwrap();
        DecisionRecord {
            schema_version: 2,
            record_type: "decision".into(),
            decision_mode: "agent".into(),
            decision_id: id.into(),
            timestamp: timestamp_now(),
            cauto_version: "test".into(),
            codex_version: "test".into(),
            repository_identifier: "repo-id".into(),
            repository_name: "repo".into(),
            git_branch: None,
            prompt_sha256: prompt_sha256(std::ffi::OsStr::new("redacted")),
            prompt_byte_length: 8,
            task_type: TaskType::Coding,
            dimensions: DimensionScores {
                scope: one,
                ambiguity: one,
                cost_of_being_wrong: one,
                runtime_dependence: one,
                architectural_depth: one,
                verification_burden: one,
                parallelizability: one,
            },
            complexity_score: 25,
            calibration: None,
            confidence_basis_points: 8_000,
            matched_rule_ids: Vec::new(),
            raising_rule_ids: Vec::new(),
            lowering_rule_ids: Vec::new(),
            conflicts: Vec::<Conflict>::new(),
            selected_model: "gpt-5.6-terra".into(),
            selected_family: ModelFamily::Terra,
            selected_effort: ReasoningLevel::Medium,
            ultra_candidate: false,
            ultra_selected: false,
            classifier_ran: false,
            classifier_outcome: "skipped".into(),
            catalog_source: CapabilitySource::Cache,
            downgrade: None,
            sanitized_argv: Vec::new(),
            feedback: None,
        }
    }

    #[test]
    fn corrections_are_high_precision_and_new_failures_are_not_feedback() {
        assert_eq!(
            implicit_bias("No, you didn't fix the reconnect bug. Try again."),
            Some(OutcomeBias::Underpowered)
        );
        assert_eq!(
            implicit_bias("That was overkill; keep it simple."),
            Some(OutcomeBias::Overkill)
        );
        assert_eq!(implicit_bias("The reconnect path is failing"), None);
        assert_eq!(implicit_bias("Please implement another feature"), None);
    }

    #[test]
    fn route_changes_have_direction_but_crossed_changes_are_ambiguous() {
        assert_eq!(
            feedback_for_route_change(
                Some(&ModelFamily::Terra),
                Some(ReasoningLevel::Medium),
                Some("gpt-sol"),
                Some(ReasoningLevel::High),
            ),
            Some(FeedbackKind::Underpowered)
        );
        assert_eq!(
            feedback_for_route_change(
                Some(&ModelFamily::Sol),
                Some(ReasoningLevel::High),
                Some("gpt-terra"),
                Some(ReasoningLevel::Medium),
            ),
            Some(FeedbackKind::Overkill)
        );
        assert_eq!(
            feedback_for_route_change(
                Some(&ModelFamily::Sol),
                Some(ReasoningLevel::Medium),
                Some("gpt-terra"),
                Some(ReasoningLevel::High),
            ),
            None
        );
    }

    #[test]
    fn prompt_extraction_ignores_non_text_inputs() {
        let params = json!({
            "input": [
                { "type": "text", "text": "first" },
                { "type": "image", "url": "ignored" },
                { "type": "text", "text": "second" }
            ]
        });
        assert_eq!(prompt_text(&params).as_deref(), Some("first\nsecond"));
    }

    #[test]
    fn collaboration_mode_uses_snake_case_authoritative_settings() {
        let mut params = json!({
            "model": "old",
            "effort": "max",
            "collaborationMode": {
                "mode": "default",
                "settings": {
                    "model": "old",
                    "reasoning_effort": "max",
                    "developer_instructions": null
                }
            }
        });
        rewrite_turn_params(
            params.as_object_mut().unwrap(),
            "gpt-terra",
            Some("medium"),
            None,
        );
        assert_eq!(params["model"], "gpt-terra");
        assert_eq!(params["effort"], "medium");
        assert_eq!(
            params["collaborationMode"]["settings"]["model"],
            "gpt-terra"
        );
        assert_eq!(
            params["collaborationMode"]["settings"]["reasoning_effort"],
            "medium"
        );
    }

    #[test]
    fn collaboration_mode_is_authoritative_when_parsing_explicit_routes() {
        let route = parse_explicit_route(&json!({
            "model": "gpt-sol",
            "effort": "max",
            "collaborationMode": {
                "settings": {
                    "model": "gpt-terra",
                    "reasoning_effort": "medium"
                }
            }
        }));
        assert_eq!(route.model.as_deref(), Some("gpt-terra"));
        assert_eq!(route.effort, Some(ReasoningLevel::Medium));
    }

    #[test]
    fn resume_response_seeds_same_thread_hysteresis_state() {
        let mut router = AgentRouter::new(
            GlobalArgs {
                repo: None,
                codex_bin: None,
                profile: None,
                json: false,
                verbose: false,
                quiet: false,
                no_color: false,
            },
            RouteArgs::default(),
            ExplicitNativeOverrides::default(),
        );
        let mut request = json!({
            "id": 7,
            "method": "thread/resume",
            "params": { "threadId": "thread-1" }
        });
        router.client_message(&mut request).unwrap();
        router
            .server_message(&json!({
                "id": 7,
                "result": {
                    "model": "gpt-5.6-sol",
                    "reasoningEffort": "high"
                }
            }))
            .unwrap();
        let state = router.threads.get("thread-1").unwrap();
        assert_eq!(state.family, Some(ModelFamily::Sol));
        assert_eq!(state.effort, Some(ReasoningLevel::High));
        assert_eq!(state.model.as_deref(), Some("gpt-5.6-sol"));
    }

    #[test]
    fn three_implicit_corrections_apply_calibration_without_manual_tuning() {
        let root = tempdir().unwrap();
        let paths = crate::paths::CautoPaths {
            config_dir: root.path().join("config"),
            cache_dir: root.path().join("cache"),
            state_dir: root.path().join("state"),
        };
        let mut last_message = None;
        for index in 0..3 {
            let id = format!("decision-{index}");
            append_json_line(
                &paths.decisions(),
                &serde_json::to_vec(&decision_record(&id)).unwrap(),
            )
            .unwrap();
            let mut state = ThreadState {
                decision_id: Some(id),
                repository_id: Some("repo-id".into()),
                repository_name: Some("repo".into()),
                ..ThreadState::default()
            };
            last_message = AgentRouter::record_feedback(
                &mut state,
                &paths,
                FeedbackKind::Underpowered,
                FeedbackSource::ImplicitCorrection,
            )
            .unwrap();
        }
        assert!(last_message.unwrap().contains("+5 points"));
        assert_eq!(
            load_store(&paths.calibration()).unwrap().repositories["repo-id"].score_offset,
            5
        );
        assert_eq!(
            build_report(&paths.decisions())
                .unwrap()
                .feedback_source_distribution["implicit-correction"],
            3
        );
    }
}
