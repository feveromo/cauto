//! Pure deterministic task scoring and route selection.

/// Confidence calculation from evidence quality.
pub mod confidence;
/// Human-readable decision explanations.
pub mod explain;
/// Bounded task feature extraction.
pub mod features;
/// Compiled project and user policy rules.
pub mod rules;
/// Weighted complexity scoring.
pub mod score;
/// Family, effort, and Ultra selection.
pub mod select;
/// Strong routing and launch-plan types.
pub mod types;

pub use confidence::{EvidenceQuality, confidence_for};
pub use features::{FeatureAssessment, extract_features};
pub use rules::{CompiledRules, RuleApplication};
pub use score::{Weights, normalized_score};
pub use select::{SelectionConstraints, route};
pub use types::*;
