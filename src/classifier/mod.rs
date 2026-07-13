//! Optional, isolated Luna second-stage classification.

pub mod prompt;
pub mod runner;
pub mod schema;

use crate::routing::DimensionScores;

pub use prompt::build_classifier_prompt;
pub use runner::{ClassifierError, ClassifierRun, should_run};
pub use schema::ClassifierAssessment;

/// Adds semantic classifier evidence without weakening deterministic policy evidence.
///
/// The classifier only runs across an explicit isolation boundary. Treating its output as a
/// monotonic supplement lets it recognize risk expressed in unfamiliar natural language while
/// preserving every deterministic rule and feature floor.
#[must_use]
pub fn blend_dimensions(
    deterministic: DimensionScores,
    classifier: &ClassifierAssessment,
) -> DimensionScores {
    DimensionScores {
        scope: deterministic.scope.max(classifier.scope),
        ambiguity: deterministic.ambiguity.max(classifier.ambiguity),
        cost_of_being_wrong: deterministic
            .cost_of_being_wrong
            .max(classifier.cost_of_being_wrong),
        runtime_dependence: deterministic
            .runtime_dependence
            .max(classifier.runtime_dependence),
        architectural_depth: deterministic
            .architectural_depth
            .max(classifier.architectural_depth),
        verification_burden: deterministic
            .verification_burden
            .max(classifier.verification_burden),
        parallelizability: deterministic
            .parallelizability
            .max(classifier.parallelizability),
    }
}
