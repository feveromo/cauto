use super::{BoundedScore, FeatureAssessment, TaskType};

fn contains_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn raise_to(score: &mut BoundedScore, floor: u8) {
    if score.get() < floor {
        *score = BoundedScore::new(floor).expect("task floors are bounded");
    }
}

fn lower_to(score: &mut BoundedScore, ceiling: u8) {
    if score.get() > ceiling {
        *score = BoundedScore::new(ceiling).expect("task budgets are bounded");
    }
}

/// Applies explicit low-cost budgets and restores evidence weakened by loose base matches.
pub(super) fn apply(mut assessment: FeatureAssessment) -> FeatureAssessment {
    if assessment.task_type == TaskType::Diagnosis && !assessment.clear_completion {
        raise_to(&mut assessment.dimensions.ambiguity, 3);
    }

    let simple_project_explanation = assessment.task_type == TaskType::Documentation
        && contains_any(
            &assessment.normalized,
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
    if simple_project_explanation {
        lower_to(&mut assessment.dimensions.scope, 1);
        lower_to(&mut assessment.dimensions.ambiguity, 1);
        lower_to(&mut assessment.dimensions.cost_of_being_wrong, 1);
        lower_to(&mut assessment.dimensions.runtime_dependence, 0);
        lower_to(&mut assessment.dimensions.architectural_depth, 0);
        lower_to(&mut assessment.dimensions.verification_burden, 1);
        assessment
            .reasons
            .retain(|reason| reason.label != "repository-wide explanation");
    }
    assessment
}
