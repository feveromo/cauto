use std::num::NonZeroU64;

use serde::Deserialize;

use crate::routing::{
    ClassifierMode, DimensionDeltas, FastMode, ModelFamily, ReasoningLevel, RuleSource, Weights,
};

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RawConfig {
    pub version: Option<u32>,
    pub classifier: Option<String>,
    pub classifier_confidence_threshold: Option<f64>,
    pub default_model: Option<String>,
    pub default_effort: Option<String>,
    pub fast_mode: Option<String>,
    pub ultra_requires_opt_in: Option<bool>,
    pub allow_automatic_downgrade: Option<bool>,
    pub log_raw_prompts: Option<bool>,
    pub strict_logging: Option<bool>,
    pub catalog_cache_hours: Option<u64>,
    pub git_timeout_ms: Option<u64>,
    pub catalog_timeout_ms: Option<u64>,
    pub classifier_timeout_seconds: Option<u64>,
    pub hysteresis_points: Option<u8>,
    #[serde(default)]
    pub weights: RawWeights,
    #[serde(default)]
    pub rules: Vec<RawRule>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct RawWeights {
    pub scope: Option<u16>,
    pub ambiguity: Option<u16>,
    pub cost_of_being_wrong: Option<u16>,
    pub runtime_dependence: Option<u16>,
    pub architectural_depth: Option<u16>,
    pub verification_burden: Option<u16>,
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct RawRule {
    pub id: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub phrases: Vec<String>,
    #[serde(default)]
    pub path_globs: Vec<String>,
    #[serde(default)]
    pub dimension_deltas: RawDimensionDeltas,
    pub family_floor: Option<String>,
    pub family_ceiling: Option<String>,
    pub effort_floor: Option<String>,
    pub effort_ceiling: Option<String>,
    #[serde(default)]
    pub confidence_delta: f64,
    #[serde(default)]
    pub reason: String,
    #[serde(skip)]
    pub source: Option<RuleSource>,
}

#[derive(Clone, Copy, Debug, Default, Deserialize)]
pub struct RawDimensionDeltas {
    #[serde(default)]
    pub scope: i8,
    #[serde(default)]
    pub ambiguity: i8,
    #[serde(default)]
    pub cost_of_being_wrong: i8,
    #[serde(default)]
    pub runtime_dependence: i8,
    #[serde(default)]
    pub architectural_depth: i8,
    #[serde(default)]
    pub verification_burden: i8,
    #[serde(default)]
    pub parallelizability: i8,
}

impl From<RawDimensionDeltas> for DimensionDeltas {
    fn from(value: RawDimensionDeltas) -> Self {
        Self {
            scope: value.scope,
            ambiguity: value.ambiguity,
            cost_of_being_wrong: value.cost_of_being_wrong,
            runtime_dependence: value.runtime_dependence,
            architectural_depth: value.architectural_depth,
            verification_burden: value.verification_burden,
            parallelizability: value.parallelizability,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeoutMillis(NonZeroU64);

impl TimeoutMillis {
    pub fn new(value: u64) -> Option<Self> {
        NonZeroU64::new(value).map(Self)
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }

    #[must_use]
    pub fn duration(self) -> std::time::Duration {
        std::time::Duration::from_millis(self.get())
    }
}

#[derive(Clone, Debug)]
pub struct ValidatedRule {
    pub id: String,
    pub description: String,
    pub phrases: Vec<String>,
    pub path_globs: Vec<String>,
    pub dimension_deltas: DimensionDeltas,
    pub family_floor: Option<ModelFamily>,
    pub family_ceiling: Option<ModelFamily>,
    pub effort_floor: Option<ReasoningLevel>,
    pub effort_ceiling: Option<ReasoningLevel>,
    pub confidence_delta_basis_points: i16,
    pub reason: String,
    pub source: RuleSource,
}

#[derive(Clone, Debug)]
pub struct ValidatedConfig {
    pub classifier: ClassifierMode,
    pub classifier_confidence_threshold_basis_points: u16,
    pub default_model: String,
    pub default_effort: ReasoningLevel,
    pub fast_mode: FastMode,
    pub ultra_requires_opt_in: bool,
    pub allow_automatic_downgrade: bool,
    pub strict_logging: bool,
    pub catalog_cache_hours: NonZeroU64,
    pub git_timeout: TimeoutMillis,
    pub catalog_timeout: TimeoutMillis,
    pub classifier_timeout: TimeoutMillis,
    pub hysteresis_points: u8,
    pub weights: Weights,
    pub rules: Vec<ValidatedRule>,
}

impl Default for ValidatedConfig {
    fn default() -> Self {
        Self {
            classifier: ClassifierMode::Auto,
            classifier_confidence_threshold_basis_points: 7_200,
            default_model: "gpt-5.6-sol".into(),
            default_effort: ReasoningLevel::Medium,
            fast_mode: FastMode::Inherit,
            ultra_requires_opt_in: true,
            allow_automatic_downgrade: true,
            strict_logging: false,
            catalog_cache_hours: NonZeroU64::new(12).expect("non-zero default"),
            git_timeout: TimeoutMillis::new(250).expect("non-zero default"),
            catalog_timeout: TimeoutMillis::new(2_500).expect("non-zero default"),
            classifier_timeout: TimeoutMillis::new(45_000).expect("non-zero default"),
            hysteresis_points: 2,
            weights: Weights::default(),
            rules: Vec::new(),
        }
    }
}
