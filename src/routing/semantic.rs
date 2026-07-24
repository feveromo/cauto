use super::{BoundedScore, DimensionScores, FeatureAssessment, Reason, ReasoningLevel, TaskType};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct EffortFloor {
    pub effort: ReasoningLevel,
    pub reason: &'static str,
}

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn contains_word(haystack: &str, needles: &[&str]) -> bool {
    haystack
        .split(|character: char| {
            !character.is_alphanumeric() && character != '\'' && character != '’'
        })
        .any(|word| needles.contains(&word))
}

fn raise_to(score: &mut BoundedScore, floor: u8) {
    if score.get() < floor {
        *score = BoundedScore::new(floor).expect("semantic floors are bounded");
    }
}

fn lower_to(score: &mut BoundedScore, ceiling: u8) {
    if score.get() > ceiling {
        *score = BoundedScore::new(ceiling).expect("semantic ceilings are bounded");
    }
}

fn push_reason(assessment: &mut FeatureAssessment, label: &str, contribution: i16) {
    if !assessment
        .reasons
        .iter()
        .any(|reason| reason.label == label)
    {
        assessment.reasons.push(Reason {
            label: label.to_owned(),
            contribution,
        });
    }
}

fn action_group_count(normalized: &str) -> usize {
    [
        contains_any(
            normalized,
            &["audit", "review", "inspect", "look over", "analyz"],
        ),
        contains_word(
            normalized,
            &[
                "add",
                "build",
                "change",
                "create",
                "fix",
                "harden",
                "implement",
                "improve",
                "remove",
                "update",
            ],
        ) || contains_any(normalized, &["make improvements", "make it better"]),
        contains_any(
            normalized,
            &["debug", "diagnose", "investigate", "reproduce"],
        ),
        contains_word(normalized, &["test", "tests", "validate", "verify"])
            || contains_any(normalized, &["acceptance criteria", "prove that"]),
        contains_word(normalized, &["benchmark", "measure", "profile"]),
        contains_word(normalized, &["document", "docs"])
            || contains_any(normalized, &["readme", "documentation"]),
        contains_word(normalized, &["migrate", "refactor", "redesign", "upgrade"]),
    ]
    .into_iter()
    .filter(|present| *present)
    .count()
}

pub(super) fn refine_features(mut assessment: FeatureAssessment) -> FeatureAssessment {
    if assessment.task_type == TaskType::Empty {
        return assessment;
    }
    let normalized = assessment.normalized.clone();
    let strong_completion = contains_any(
        &normalized,
        &[
            "acceptance criteria",
            "expected behavior",
            "expected output",
            "success criteria",
            "done when",
            "must pass",
            "all tests pass",
            "command exits 0",
            "verify that",
            "prove that",
            "provided test",
            "provided fixture",
            "provided reproduction",
        ],
    );
    if assessment.clear_completion && !strong_completion {
        assessment.clear_completion = false;
        assessment.dimensions.verification_burden = assessment
            .dimensions
            .verification_burden
            .saturating_add_signed(-1);
    }

    let project_explanation = contains_any(
        &normalized,
        &[
            "what is this project",
            "what does this project do",
            "explain this project",
            "explain the project",
            "explain this codebase",
            "how does this project work",
            "project overview",
            "explain to me like",
            "explain it like i'm",
            "explain it like im",
        ],
    );
    let docs_signal = project_explanation
        || contains_any(
            &normalized,
            &[
                "documentation",
                "readme",
                "markdown",
                "fix typo",
                "spelling",
            ],
        );
    let failure_signal = contains_word(
        &normalized,
        &[
            "broken", "crash", "crashed", "error", "failed", "failing", "failure", "cannot",
            "can't", "cant", "can’t", "won't", "wont", "won’t",
        ],
    ) || contains_any(
        &normalized,
        &[
            "not working",
            "doesn't work",
            "doesnt work",
            "bugging out",
            "regression",
        ],
    );
    let diagnosis_signal = failure_signal
        || contains_any(
            &normalized,
            &[
                "debug why",
                "diagnose",
                "investigate",
                "intermittent",
                "root cause",
            ],
        );
    let research_signal = contains_any(
        &normalized,
        &[
            "attack surface",
            "deobfuscat",
            "exploit",
            "exploitable",
            "research",
            "reverse engineer",
            "reconstruct",
            "security audit",
            "security review",
        ],
    );
    let architecture_signal = contains_any(
        &normalized,
        &[
            "architecture",
            "across subsystems",
            "cross-system",
            "lifecycle",
            "protocol",
            "redesign",
            "system design",
        ],
    );
    let implementation_signal = contains_word(
        &normalized,
        &[
            "add",
            "build",
            "change",
            "create",
            "fix",
            "harden",
            "implement",
            "improve",
            "migrate",
            "optimize",
            "refactor",
            "remove",
            "update",
            "upgrade",
        ],
    ) || contains_any(
        &normalized,
        &[
            "make improvements",
            "make it better",
            "make all of it better",
        ],
    );
    let review_signal = contains_word(&normalized, &["audit", "review"])
        || contains_any(&normalized, &["look over", "code review"]);
    let mechanical_signal = contains_any(
        &normalized,
        &[
            "formatting",
            "regenerate",
            "rename",
            "sort imports",
            "sync catalog",
            "synchronize",
        ],
    );
    let repository_subject = contains_word(
        &normalized,
        &["codebase", "project", "repo", "repository", "router"],
    ) || contains_any(
        &normalized,
        &["routing engine", "whole workspace", "entire workspace"],
    );
    let broad_improvement = repository_subject
        && contains_any(
            &normalized,
            &[
                "audit",
                "clean up",
                "cleanup",
                "harden",
                "improve",
                "look over",
                "make improvements",
                "make it better",
                "optimize",
                "polish",
                "review",
            ],
        )
        && !(docs_signal && !project_explanation && !diagnosis_signal && !architecture_signal);
    let broad_change = contains_any(
        &normalized,
        &[
            "across the codebase",
            "across the repo",
            "across the repository",
            "all call sites",
            "cross-platform",
            "end-to-end",
            "multiple crates",
            "multiple packages",
            "repo-wide",
            "repository-wide",
            "workspace-wide",
        ],
    ) || contains_word(&normalized, &["migration", "refactor", "redesign"]);
    let performance_work = contains_word(
        &normalized,
        &[
            "benchmark",
            "latency",
            "performance",
            "profile",
            "throughput",
        ],
    );
    let correctness_work = contains_any(
        &normalized,
        &[
            "backward compatibility",
            "backwards compatibility",
            "data loss",
            "schema migration",
            "serialization",
            "state corruption",
            "transaction",
        ],
    );
    let platform_work = contains_any(
        &normalized,
        &[
            "cross-platform",
            "linux and mac",
            "linux and macos",
            "mac and linux",
            "macos and linux",
        ],
    );
    let security_work = contains_any(
        &normalized,
        &[
            "authentication",
            "authorization",
            "credential rotation",
            "oauth",
            "permission model",
            "production credentials",
            "secret handling",
            "security-sensitive",
            "security sensitive",
        ],
    ) && !contains_any(&normalized, &["example", "mock", "test fixture"]);
    let concurrency_work = contains_any(
        &normalized,
        &[
            "atomicity",
            "concurrency",
            "deadlock",
            "race condition",
            "thread safety",
            "thread-safe",
        ],
    );
    let action_groups = action_group_count(&normalized);

    if research_signal {
        assessment.task_type = TaskType::Research;
    } else if architecture_signal && implementation_signal {
        assessment.task_type = TaskType::Architecture;
    } else if diagnosis_signal {
        assessment.task_type = TaskType::Diagnosis;
    } else if broad_improvement || (review_signal && implementation_signal) {
        assessment.task_type = TaskType::Coding;
    } else if review_signal {
        assessment.task_type = TaskType::Review;
    } else if mechanical_signal && !architecture_signal {
        assessment.task_type = TaskType::Mechanical;
    } else if docs_signal && !implementation_signal {
        assessment.task_type = TaskType::Documentation;
    }

    if project_explanation {
        raise_to(&mut assessment.dimensions.scope, 2);
        raise_to(&mut assessment.dimensions.architectural_depth, 2);
        raise_to(&mut assessment.dimensions.verification_burden, 2);
        push_reason(&mut assessment, "repository-wide explanation", 10);
    }
    if broad_improvement {
        raise_to(&mut assessment.dimensions.scope, 3);
        raise_to(&mut assessment.dimensions.ambiguity, 3);
        raise_to(&mut assessment.dimensions.architectural_depth, 2);
        raise_to(&mut assessment.dimensions.verification_burden, 3);
        push_reason(&mut assessment, "open-ended repository improvement", 18);
    }
    if broad_change {
        raise_to(&mut assessment.dimensions.scope, 3);
        raise_to(&mut assessment.dimensions.architectural_depth, 2);
        raise_to(&mut assessment.dimensions.verification_burden, 2);
        push_reason(&mut assessment, "cross-cutting change", 14);
    }
    if performance_work {
        raise_to(&mut assessment.dimensions.runtime_dependence, 2);
        raise_to(&mut assessment.dimensions.verification_burden, 3);
        push_reason(&mut assessment, "performance requires measurement", 12);
    }
    if correctness_work {
        raise_to(&mut assessment.dimensions.cost_of_being_wrong, 3);
        raise_to(&mut assessment.dimensions.verification_burden, 3);
        push_reason(&mut assessment, "state or compatibility correctness", 15);
    }
    if platform_work {
        raise_to(&mut assessment.dimensions.scope, 3);
        raise_to(&mut assessment.dimensions.runtime_dependence, 2);
        raise_to(&mut assessment.dimensions.verification_burden, 3);
        push_reason(&mut assessment, "cross-platform behavior", 15);
    }
    if security_work {
        raise_to(&mut assessment.dimensions.cost_of_being_wrong, 3);
        raise_to(&mut assessment.dimensions.verification_burden, 3);
        push_reason(&mut assessment, "security-sensitive behavior", 18);
    }
    if concurrency_work {
        raise_to(&mut assessment.dimensions.cost_of_being_wrong, 3);
        raise_to(&mut assessment.dimensions.runtime_dependence, 2);
        raise_to(&mut assessment.dimensions.architectural_depth, 2);
        raise_to(&mut assessment.dimensions.verification_burden, 3);
        push_reason(&mut assessment, "concurrency correctness", 18);
    }
    match assessment.explicit_paths.len() {
        0 | 1 => {}
        2..=4 => raise_to(&mut assessment.dimensions.scope, 2),
        _ => raise_to(&mut assessment.dimensions.scope, 3),
    }
    if action_groups >= 3 {
        raise_to(&mut assessment.dimensions.scope, 2);
        raise_to(&mut assessment.dimensions.verification_burden, 2);
        push_reason(
            &mut assessment,
            "compound implementation and verification",
            10,
        );
    }
    if action_groups >= 4 {
        raise_to(&mut assessment.dimensions.scope, 3);
        raise_to(&mut assessment.dimensions.verification_burden, 3);
        raise_to(&mut assessment.dimensions.parallelizability, 2);
    }
    if assessment.vague_prompt
        && assessment.task_type != TaskType::Documentation
        && assessment.task_type != TaskType::Mechanical
    {
        raise_to(&mut assessment.dimensions.ambiguity, 2);
    }

    let narrow_mechanical = assessment.task_type == TaskType::Mechanical
        && assessment.explicit_paths.len() <= 2
        && action_groups <= 2
        && !diagnosis_signal
        && !broad_change;
    if narrow_mechanical {
        lower_to(&mut assessment.dimensions.scope, 1);
        lower_to(&mut assessment.dimensions.ambiguity, 1);
        lower_to(&mut assessment.dimensions.cost_of_being_wrong, 1);
        lower_to(&mut assessment.dimensions.runtime_dependence, 0);
        lower_to(&mut assessment.dimensions.architectural_depth, 0);
        lower_to(&mut assessment.dimensions.verification_burden, 1);
        push_reason(&mut assessment, "bounded mechanical change", -12);
    }

    assessment
}

pub(super) fn risk_effort_floor(
    task_type: &TaskType,
    dimensions: DimensionScores,
) -> Option<EffortFloor> {
    let severe = [
        dimensions.ambiguity.get(),
        dimensions.cost_of_being_wrong.get(),
        dimensions.runtime_dependence.get(),
        dimensions.architectural_depth.get(),
        dimensions.verification_burden.get(),
    ];
    let severe_count = severe.into_iter().filter(|value| *value >= 3).count();
    if dimensions.cost_of_being_wrong.get() >= 4
        || dimensions.runtime_dependence.get() >= 4
        || dimensions.verification_burden.get() >= 4
    {
        return Some(EffortFloor {
            effort: ReasoningLevel::High,
            reason: "critical risk requires at least High reasoning",
        });
    }
    if dimensions.cost_of_being_wrong.get() >= 3
        && (dimensions.ambiguity.get() >= 2
            || dimensions.runtime_dependence.get() >= 2
            || dimensions.verification_burden.get() >= 2)
    {
        return Some(EffortFloor {
            effort: ReasoningLevel::High,
            reason: "high-consequence work requires at least High reasoning",
        });
    }
    if severe_count >= 2 {
        return Some(EffortFloor {
            effort: ReasoningLevel::High,
            reason: "multiple high-risk dimensions require at least High reasoning",
        });
    }
    if (task_type == &TaskType::Research || task_type == &TaskType::Architecture)
        && (dimensions.ambiguity.get() >= 3 || dimensions.architectural_depth.get() >= 3)
    {
        return Some(EffortFloor {
            effort: ReasoningLevel::High,
            reason: "research or architecture requires at least High reasoning",
        });
    }
    if task_type == &TaskType::Diagnosis
        && dimensions.ambiguity.get() >= 3
        && (dimensions.runtime_dependence.get() >= 2 || dimensions.verification_burden.get() >= 2)
    {
        return Some(EffortFloor {
            effort: ReasoningLevel::High,
            reason: "uncertain diagnosis requires at least High reasoning",
        });
    }
    None
}
