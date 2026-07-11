use super::{BoundedScore, DimensionScores, EscalationSignal, Reason, TaskType};

/// A normalized, allocation-bounded deterministic view of the task.
#[derive(Clone, Debug)]
pub struct FeatureAssessment {
    /// Single lowercase buffer used by all deterministic matchers.
    pub normalized: String,
    /// Dimension scores inferred from generic task evidence.
    pub dimensions: DimensionScores,
    /// Broad task category inferred from the task text.
    pub task_type: TaskType,
    /// Bounded list of path-like tokens mentioned by the task.
    pub explicit_paths: Vec<String>,
    /// Whether an explicit reproduction is present.
    pub clear_reproduction: bool,
    /// Whether an explicit completion condition is present.
    pub clear_completion: bool,
    /// Whether the task lacks enough detail for high confidence.
    pub vague_prompt: bool,
    /// Whether the user explicitly requested delegation.
    pub delegation_requested: bool,
    /// Whether the described work contains independent parallel tracks.
    pub meaningful_parallel_tracks: bool,
    /// Explainable generic scoring contributions.
    pub reasons: Vec<Reason>,
    /// Safety or complexity signals that can require escalation.
    pub escalation_signals: Vec<EscalationSignal>,
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn raise_to(score: &mut BoundedScore, floor: u8) {
    if score.get() < floor {
        *score = BoundedScore::new(floor).expect("feature floors are bounded");
    }
}

fn lower_to(score: &mut BoundedScore, ceiling: u8) {
    if score.get() > ceiling {
        *score = BoundedScore::new(ceiling).expect("feature ceilings are bounded");
    }
}

/// Extracts generic evidence from one lowercase prompt buffer.
#[must_use]
pub fn extract_features(prompt: &str) -> FeatureAssessment {
    let normalized = prompt.to_lowercase();
    if normalized.trim().is_empty() {
        return FeatureAssessment {
            normalized,
            dimensions: DimensionScores::default(),
            task_type: TaskType::Empty,
            explicit_paths: Vec::new(),
            clear_reproduction: false,
            clear_completion: false,
            vague_prompt: true,
            delegation_requested: false,
            meaningful_parallel_tracks: false,
            reasons: vec![Reason {
                label: "no task supplied; using configured default".into(),
                contribution: 0,
            }],
            escalation_signals: Vec::new(),
        };
    }

    let mut dimensions = DimensionScores::default();
    let mut reasons = Vec::with_capacity(10);
    let mut escalation_signals = Vec::with_capacity(6);
    let explicit_paths = extract_paths(&normalized);
    let clear_reproduction = contains_any(
        &normalized,
        &[
            "exact repro",
            "reproduction",
            "steps to reproduce",
            "repro:",
        ],
    );
    let clear_completion = contains_any(
        &normalized,
        &[
            "acceptance criteria",
            "expected behavior",
            "expected output",
            "prove that",
            "provided",
        ],
    );
    let docs = contains_any(
        &normalized,
        &[
            "documentation",
            "readme",
            "markdown",
            "fix typo",
            "spelling",
        ],
    );
    let mechanical = contains_any(
        &normalized,
        &[
            "rename",
            "formatting",
            "regenerate",
            "synchronize",
            "sync catalog",
        ],
    );
    let diagnosis = contains_any(
        &normalized,
        &[
            "diagnose",
            "debug why",
            "unknown root cause",
            "investigate",
            "intermittent",
        ],
    );
    let research = contains_any(
        &normalized,
        &["reverse engineer", "deobfuscat", "research", "reconstruct"],
    );
    let architecture = contains_any(
        &normalized,
        &[
            "architecture",
            "redesign",
            "across subsystems",
            "cross-system",
            "lifecycle",
            "protocol",
        ],
    );
    let review = normalized.contains("review") && !normalized.contains("reviewed");

    if normalized.len() > 1_200 {
        dimensions.scope = dimensions.scope.saturating_add_signed(1);
        reasons.push(Reason {
            label: "long task specification".into(),
            contribution: 5,
        });
    }
    if normalized.len() > 5_000 || explicit_paths.len() >= 5 {
        dimensions.scope = dimensions.scope.saturating_add_signed(1);
    }
    if clear_reproduction {
        dimensions.ambiguity = dimensions.ambiguity.saturating_add_signed(-1);
        reasons.push(Reason {
            label: "clear reproduction".into(),
            contribution: -7,
        });
    }
    if clear_completion {
        dimensions.verification_burden = dimensions.verification_burden.saturating_add_signed(1);
    }
    if diagnosis {
        raise_to(&mut dimensions.ambiguity, 3);
        reasons.push(Reason {
            label: "root-cause investigation".into(),
            contribution: 12,
        });
    }
    if research {
        raise_to(&mut dimensions.ambiguity, 4);
        raise_to(&mut dimensions.architectural_depth, 3);
        escalation_signals.push(EscalationSignal {
            label: "research or reverse engineering".into(),
        });
    }
    if architecture {
        raise_to(&mut dimensions.scope, 3);
        raise_to(&mut dimensions.architectural_depth, 3);
        reasons.push(Reason {
            label: "architectural boundary".into(),
            contribution: 10,
        });
    }
    if contains_any(
        &normalized,
        &[
            "live client",
            "live-validate",
            "live validate",
            "session gate",
            "restart",
            "ui",
            "browser",
            "desktop",
            "runtime state",
        ],
    ) {
        raise_to(&mut dimensions.runtime_dependence, 3);
        raise_to(&mut dimensions.cost_of_being_wrong, 3);
        raise_to(&mut dimensions.verification_burden, 3);
        reasons.push(Reason {
            label: "live runtime validation".into(),
            contribution: 15,
        });
    }
    if contains_any(
        &normalized,
        &[
            "security-sensitive",
            "security sensitive",
            "production",
            "account-affecting",
            "destructive",
            "credentials",
            "race condition",
            "concurrency",
        ],
    ) {
        raise_to(&mut dimensions.cost_of_being_wrong, 3);
        escalation_signals.push(EscalationSignal {
            label: "high cost of being wrong".into(),
        });
    }
    if contains_any(
        &normalized,
        &[
            "reflection",
            "packet",
            "renderer",
            "projection",
            "coordinate system",
            "pathing",
            "queue",
        ],
    ) {
        raise_to(&mut dimensions.architectural_depth, 3);
    }
    if contains_any(
        &normalized,
        &[
            "run tests",
            "run contracts",
            "full check",
            "instrumentation",
            "prove vertices",
            "repeated experiments",
            "failure recovery",
        ],
    ) {
        raise_to(&mut dimensions.verification_burden, 2);
    }

    let delegation_requested = contains_any(
        &normalized,
        &[
            "parallel agents",
            "subagents",
            "sub-agents",
            "delegate to agents",
            "use agents in parallel",
        ],
    );
    let meaningful_parallel_tracks = contains_any(
        &normalized,
        &[
            "independent tracks",
            "parallel tracks",
            "separate workstreams",
            "implementation, validation, and documentation",
            "repository audit",
        ],
    );
    if delegation_requested {
        raise_to(&mut dimensions.parallelizability, 3);
    }
    if meaningful_parallel_tracks {
        raise_to(&mut dimensions.parallelizability, 3);
    }
    if normalized.contains("three independent") || normalized.contains("several independent") {
        raise_to(&mut dimensions.parallelizability, 4);
    }

    if docs && !diagnosis && !architecture && dimensions.runtime_dependence.get() == 0 {
        lower_to(&mut dimensions.scope, 1);
        lower_to(&mut dimensions.ambiguity, 1);
        lower_to(&mut dimensions.cost_of_being_wrong, 1);
        lower_to(&mut dimensions.architectural_depth, 0);
        lower_to(&mut dimensions.verification_burden, 1);
    }
    if mechanical && clear_completion {
        lower_to(&mut dimensions.ambiguity, 1);
    }

    let vague_prompt = normalized.split_whitespace().count() < 4
        || contains_any(
            &normalized,
            &["fix it", "make it work", "something is wrong"],
        );
    let task_type = if docs {
        TaskType::Documentation
    } else if research {
        TaskType::Research
    } else if architecture {
        TaskType::Architecture
    } else if diagnosis {
        TaskType::Diagnosis
    } else if review {
        TaskType::Review
    } else if mechanical {
        TaskType::Mechanical
    } else {
        TaskType::Coding
    };

    FeatureAssessment {
        normalized,
        dimensions,
        task_type,
        explicit_paths,
        clear_reproduction,
        clear_completion,
        vague_prompt,
        delegation_requested,
        meaningful_parallel_tracks,
        reasons,
        escalation_signals,
    }
}

fn extract_paths(prompt: &str) -> Vec<String> {
    let mut paths = Vec::with_capacity(4);
    for token in prompt.split_whitespace() {
        let candidate = token.trim_matches(|character: char| {
            matches!(
                character,
                '`' | '"' | '\'' | '(' | ')' | '[' | ']' | ',' | ';' | ':'
            )
        });
        let looks_like_path = candidate.contains('/')
            || [
                ".rs", ".toml", ".md", ".java", ".json", ".yaml", ".yml", ".js", ".ts", ".py",
            ]
            .iter()
            .any(|extension| candidate.ends_with(extension));
        if looks_like_path
            && candidate.len() <= 512
            && !candidate.contains("://")
            && !paths.iter().any(|existing| existing == candidate)
        {
            paths.push(candidate.to_owned());
            if paths.len() == 32 {
                break;
            }
        }
    }
    paths
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_prompt_is_not_classified_as_cheap_work() {
        let assessment = extract_features("");
        assert_eq!(assessment.task_type, TaskType::Empty);
        assert!(assessment.vague_prompt);
    }

    #[test]
    fn live_reverse_engineering_raises_risk() {
        let assessment = extract_features(
            "reverse engineer packet pathing, restart, and live-validate with repeated experiments",
        );
        assert_eq!(assessment.dimensions.ambiguity.get(), 4);
        assert!(assessment.dimensions.runtime_dependence.get() >= 3);
        assert!(assessment.dimensions.architectural_depth.get() >= 3);
    }
}
