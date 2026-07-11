//! Optional, isolated Luna second-stage classification.

pub mod prompt;
pub mod runner;
pub mod schema;

use crate::routing::{BoundedScore, DimensionScores};

pub use prompt::build_classifier_prompt;
pub use runner::{ClassifierError, ClassifierRun, should_run};
pub use schema::ClassifierAssessment;

fn blend(left: BoundedScore, right: BoundedScore) -> BoundedScore {
    let value = (u16::from(left.get()) * 7 + u16::from(right.get()) * 3 + 5) / 10;
    BoundedScore::new(value as u8).expect("weighted bounded scores remain bounded")
}

/// Blends classifier evidence at 30%, retaining deterministic policy as the majority.
#[must_use]
pub fn blend_dimensions(
    deterministic: DimensionScores,
    classifier: &ClassifierAssessment,
) -> DimensionScores {
    DimensionScores {
        scope: blend(deterministic.scope, classifier.scope),
        ambiguity: blend(deterministic.ambiguity, classifier.ambiguity),
        cost_of_being_wrong: blend(
            deterministic.cost_of_being_wrong,
            classifier.cost_of_being_wrong,
        ),
        runtime_dependence: blend(
            deterministic.runtime_dependence,
            classifier.runtime_dependence,
        ),
        architectural_depth: blend(
            deterministic.architectural_depth,
            classifier.architectural_depth,
        ),
        verification_burden: blend(
            deterministic.verification_burden,
            classifier.verification_burden,
        ),
        parallelizability: blend(
            deterministic.parallelizability,
            classifier.parallelizability,
        ),
    }
}
