use std::ffi::OsString;
use std::fmt::{Display, Formatter};
use std::path::PathBuf;
use std::str::FromStr;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A broad Codex model family ordered from least to most capable.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ModelFamily {
    /// Lowest-cost family for mechanical, well-specified work.
    Luna,
    /// General-purpose family for bounded implementation work.
    Terra,
    /// Highest-capability family for ambiguous or high-risk work.
    Sol,
    /// A catalog model outside the built-in family taxonomy.
    Other(String),
}

impl ModelFamily {
    /// Returns the family capability rank used for floors and ceilings.
    #[must_use]
    pub const fn rank(&self) -> u8 {
        match self {
            Self::Luna => 0,
            Self::Terra => 1,
            Self::Sol | Self::Other(_) => 2,
        }
    }

    /// Infers a family from a catalog model identifier.
    #[must_use]
    pub fn from_model_id(model: &str) -> Self {
        let lower = model.to_ascii_lowercase();
        if lower.contains("luna") {
            Self::Luna
        } else if lower.contains("terra") {
            Self::Terra
        } else if lower.contains("sol") {
            Self::Sol
        } else {
            Self::Other(model.to_owned())
        }
    }
}

impl Display for ModelFamily {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Luna => f.write_str("luna"),
            Self::Terra => f.write_str("terra"),
            Self::Sol => f.write_str("sol"),
            Self::Other(value) => f.write_str(value),
        }
    }
}

impl FromStr for ModelFamily {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "luna" => Ok(Self::Luna),
            "terra" => Ok(Self::Terra),
            "sol" => Ok(Self::Sol),
            _ => Err(format!("unknown model family {value:?}")),
        }
    }
}

impl Serialize for ModelFamily {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for ModelFamily {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Ok(match value.as_str() {
            "luna" => Self::Luna,
            "terra" => Self::Terra,
            "sol" => Self::Sol,
            _ => Self::Other(value),
        })
    }
}

/// User-facing reasoning levels, including catalog-proven Max and Ultra.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ReasoningLevel {
    /// Minimal native reasoning, when exposed by a model.
    Minimal,
    /// Low reasoning for clear, inexpensive work.
    Low,
    /// Medium reasoning for ordinary bounded coding.
    Medium,
    /// High reasoning for ambiguous or runtime-dependent work.
    High,
    /// Native `xhigh` reasoning.
    ExtraHigh,
    /// Catalog-proven native Max reasoning.
    Max,
    /// Catalog-proven native Ultra reasoning with proactive delegation.
    Ultra,
}

impl ReasoningLevel {
    /// Returns the exact native Codex config value.
    #[must_use]
    pub const fn native_name(self) -> &'static str {
        match self {
            Self::Minimal => "minimal",
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::ExtraHigh => "xhigh",
            Self::Max => "max",
            Self::Ultra => "ultra",
        }
    }

    /// Returns a concise human display label.
    #[must_use]
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Minimal => "Minimal",
            Self::Low => "Low",
            Self::Medium => "Medium",
            Self::High => "High",
            Self::ExtraHigh => "Extra High",
            Self::Max => "Max",
            Self::Ultra => "Ultra",
        }
    }
}

impl Display for ReasoningLevel {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.native_name())
    }
}

impl FromStr for ReasoningLevel {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().replace([' ', '-'], "_").as_str() {
            "minimal" => Ok(Self::Minimal),
            "low" | "light" => Ok(Self::Low),
            "medium" => Ok(Self::Medium),
            "high" => Ok(Self::High),
            "xhigh" | "extra_high" => Ok(Self::ExtraHigh),
            "max" => Ok(Self::Max),
            "ultra" => Ok(Self::Ultra),
            _ => Err(format!("unknown reasoning effort {value:?}")),
        }
    }
}

impl Serialize for ReasoningLevel {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.native_name())
    }
}

impl<'de> Deserialize<'de> for ReasoningLevel {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        value.parse().map_err(serde::de::Error::custom)
    }
}

/// Controls optional second-stage classifier invocation.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ClassifierMode {
    /// Run only when deterministic confidence or conflicts warrant it.
    #[default]
    Auto,
    /// Run whenever the classifier boundary is otherwise usable.
    Always,
    /// Never run the classifier.
    Never,
}

impl FromStr for ClassifierMode {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "auto" => Ok(Self::Auto),
            "always" => Ok(Self::Always),
            "never" => Ok(Self::Never),
            _ => Err(format!("unknown classifier mode {value:?}")),
        }
    }
}

/// Native Codex launch surface.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum LaunchMode {
    /// Interactive native Codex TUI.
    Interactive,
    /// Non-interactive native `codex exec`.
    Exec,
}

/// Per-launch Fast service-tier behavior.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FastMode {
    /// Explicitly request the installed Fast tier.
    Fast,
    /// Explicitly request the normal tier.
    NoFast,
    /// Preserve the user's existing Codex setting.
    #[default]
    Inherit,
}

/// A validated inclusive score from zero through four.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct BoundedScore(u8);

impl BoundedScore {
    /// Smallest valid dimension score.
    pub const MIN: u8 = 0;
    /// Largest valid dimension score.
    pub const MAX: u8 = 4;

    /// Validates and constructs a dimension score.
    pub fn new(value: u8) -> Result<Self, String> {
        if value <= Self::MAX {
            Ok(Self(value))
        } else {
            Err(format!("score {value} is outside 0..=4"))
        }
    }

    /// Returns the validated integer value.
    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    /// Applies a signed delta while preserving the 0..=4 invariant.
    #[must_use]
    pub fn saturating_add_signed(self, delta: i8) -> Self {
        let value = i16::from(self.0) + i16::from(delta);
        Self(value.clamp(0, 4) as u8)
    }
}

impl TryFrom<u8> for BoundedScore {
    type Error = String;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl<'de> Deserialize<'de> for BoundedScore {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        Self::new(u8::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

/// Confidence represented as deterministic basis points in the range 0..=10,000.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(transparent)]
pub struct Confidence(u16);

impl Confidence {
    /// Largest valid confidence value in basis points.
    pub const MAX_BASIS_POINTS: u16 = 10_000;

    /// Validates and constructs confidence from basis points.
    pub fn from_basis_points(value: u16) -> Result<Self, String> {
        if value <= Self::MAX_BASIS_POINTS {
            Ok(Self(value))
        } else {
            Err(format!("confidence {value}bp is outside 0..=10000"))
        }
    }

    /// Validates and constructs confidence from a 0.0..=1.0 ratio.
    pub fn from_ratio(value: f64) -> Result<Self, String> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(format!("confidence {value} is outside 0.0..=1.0"));
        }
        Self::from_basis_points((value * 10_000.0).round() as u16)
    }

    /// Returns confidence in deterministic basis points.
    #[must_use]
    pub const fn basis_points(self) -> u16 {
        self.0
    }

    /// Returns confidence as a human-oriented ratio.
    #[must_use]
    pub fn ratio(self) -> f64 {
        f64::from(self.0) / 10_000.0
    }
}

impl Default for Confidence {
    fn default() -> Self {
        Self(5_000)
    }
}

/// The seven bounded routing dimensions for one task.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct DimensionScores {
    /// Breadth of the requested change.
    pub scope: BoundedScore,
    /// Amount of investigation or uncertainty.
    pub ambiguity: BoundedScore,
    /// Consequence of an incorrect result.
    pub cost_of_being_wrong: BoundedScore,
    /// Dependence on live or process-level behavior.
    pub runtime_dependence: BoundedScore,
    /// Depth of the affected architectural boundary.
    pub architectural_depth: BoundedScore,
    /// Work required to prove the result.
    pub verification_burden: BoundedScore,
    /// Degree to which independent tracks can run in parallel.
    pub parallelizability: BoundedScore,
}

impl Default for DimensionScores {
    fn default() -> Self {
        let one = BoundedScore(1);
        Self {
            scope: one,
            ambiguity: one,
            cost_of_being_wrong: one,
            runtime_dependence: BoundedScore(0),
            architectural_depth: one,
            verification_burden: one,
            parallelizability: BoundedScore(0),
        }
    }
}

impl DimensionScores {
    /// Applies a validated rule's signed deltas to every dimension.
    pub fn apply(&mut self, deltas: DimensionDeltas) {
        self.scope = self.scope.saturating_add_signed(deltas.scope);
        self.ambiguity = self.ambiguity.saturating_add_signed(deltas.ambiguity);
        self.cost_of_being_wrong = self
            .cost_of_being_wrong
            .saturating_add_signed(deltas.cost_of_being_wrong);
        self.runtime_dependence = self
            .runtime_dependence
            .saturating_add_signed(deltas.runtime_dependence);
        self.architectural_depth = self
            .architectural_depth
            .saturating_add_signed(deltas.architectural_depth);
        self.verification_burden = self
            .verification_burden
            .saturating_add_signed(deltas.verification_burden);
        self.parallelizability = self
            .parallelizability
            .saturating_add_signed(deltas.parallelizability);
    }
}

/// Signed per-dimension adjustments supplied by a policy rule.
#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct DimensionDeltas {
    /// Scope adjustment.
    pub scope: i8,
    /// Ambiguity adjustment.
    pub ambiguity: i8,
    /// Cost-of-error adjustment.
    pub cost_of_being_wrong: i8,
    /// Runtime-dependence adjustment.
    pub runtime_dependence: i8,
    /// Architectural-depth adjustment.
    pub architectural_depth: i8,
    /// Verification-burden adjustment.
    pub verification_burden: i8,
    /// Parallelizability adjustment.
    pub parallelizability: i8,
}

/// Broad deterministic task category used in decisions and redacted history.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum TaskType {
    /// No task text was supplied.
    Empty,
    /// Documentation-only work.
    Documentation,
    /// Mechanical transformation or rename.
    Mechanical,
    /// Bounded implementation work.
    Coding,
    /// Root-cause diagnosis.
    Diagnosis,
    /// Architectural design or redesign.
    Architecture,
    /// Research or reverse engineering.
    Research,
    /// Review without an implied implementation.
    Review,
}

/// Repository-local score calibration, bounded to a conservative range.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct ScoreCalibration(i8);

impl ScoreCalibration {
    /// Largest positive or negative repository calibration.
    pub const MAX_ABS: i8 = 10;

    /// Validates a persisted or proposed calibration value.
    pub fn new(points: i8) -> Result<Self, String> {
        if (-Self::MAX_ABS..=Self::MAX_ABS).contains(&points) {
            Ok(Self(points))
        } else {
            Err(format!(
                "calibration {points} is outside -{}..={}",
                Self::MAX_ABS,
                Self::MAX_ABS
            ))
        }
    }

    /// Returns the signed score-point offset.
    #[must_use]
    pub const fn points(self) -> i8 {
        self.0
    }

    /// Returns whether the calibration has no routing effect.
    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0 == 0
    }
}

/// Separate explanation of how repository calibration affected selection.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalibrationEffect {
    /// Persisted bounded offset considered by the router.
    pub configured_offset: i8,
    /// Offset actually applied after task and Max guards.
    pub applied_offset: i8,
    /// Deterministic score before repository calibration.
    pub base_score: u8,
    /// Score passed into family, effort, and hysteresis selection.
    pub calibrated_score: u8,
    /// Exact explanation for applying or suppressing the offset.
    pub reason: String,
}

/// Configuration layer from which a rule originated.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleSource {
    /// Generic rule compiled into cauto.
    Builtin,
    /// User-level configuration rule.
    User,
    /// Repository-level project rule.
    Project,
}

/// Structured evidence retained for one matched rule.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RuleMatch {
    /// Stable rule identifier.
    pub rule_id: String,
    /// Configuration layer that defined the rule.
    pub source: RuleSource,
    /// Concise phrase or path that caused the match.
    pub matched_text_or_path: String,
    /// Dimension adjustments contributed by the rule.
    pub dimension_effects: DimensionDeltas,
    /// Optional model-family safety floor.
    pub family_floor: Option<ModelFamily>,
    /// Optional model-family cost ceiling.
    pub family_ceiling: Option<ModelFamily>,
    /// Optional reasoning safety floor.
    pub effort_floor: Option<ReasoningLevel>,
    /// Optional reasoning cost ceiling.
    pub effort_ceiling: Option<ReasoningLevel>,
    /// Signed confidence adjustment in basis points.
    pub confidence_effect_basis_points: i16,
    /// Human explanation supplied by the policy.
    pub reason: String,
}

/// An explicit incompatibility among matched routing constraints.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Conflict {
    /// Stable machine-readable conflict category.
    pub kind: String,
    /// Concise human-readable explanation.
    pub message: String,
}

/// One explainable contribution to a routing decision.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Reason {
    /// Concise evidence label.
    pub label: String,
    /// Signed relative contribution used for explanation ordering.
    pub contribution: i16,
}

/// A high-risk or high-complexity signal retained for diagnostics.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct EscalationSignal {
    /// Concise signal label.
    pub label: String,
}

/// Complete deterministic recommendation before catalog resolution.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RouteDecision {
    /// Broad task category.
    pub task_type: TaskType,
    /// Final dimension scores after policy application.
    pub dimensions: DimensionScores,
    /// Weighted complexity normalized to 0..=100.
    pub normalized_score: u8,
    /// Repository calibration considered during selection, when configured.
    pub calibration: Option<CalibrationEffect>,
    /// Confidence in the evidence, independent of model strength.
    pub confidence: Confidence,
    /// Structured project and user rule matches.
    pub matched_rules: Vec<RuleMatch>,
    /// Incompatible routing constraints.
    pub conflicts: Vec<Conflict>,
    /// Lowest family expected to complete the task reliably.
    pub recommended_family: ModelFamily,
    /// Recommended native reasoning level.
    pub recommended_effort: ReasoningLevel,
    /// Whether complexity and task structure qualify for Ultra.
    pub ultra_candidate: bool,
    /// Whether Ultra was both eligible and explicitly authorized.
    pub ultra_selected: bool,
    /// Explainable generic and policy contributions.
    pub reasons: Vec<Reason>,
    /// Retained escalation evidence.
    pub escalation_signals: Vec<EscalationSignal>,
}

/// Provenance of a model capability entry.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum CapabilitySource {
    /// Validated versioned local cache.
    Cache,
    /// Live `codex debug models` output.
    DebugModels,
    /// Live bundled catalog output.
    Bundled,
    /// Codex App Server discovery.
    AppServer,
    /// Conservative built-in emergency metadata.
    Fallback,
}

/// Native collaboration mode when a preset requires one.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum CollaborationMode {
    /// Normal interactive collaboration.
    Default,
    /// Planning collaboration mode.
    Plan,
}

/// Catalog-resolved model and reasoning preset for a launch path.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RoutePreset {
    /// Exact native model identifier.
    pub model_id: String,
    /// Router family classification.
    pub model_family: ModelFamily,
    /// User-facing resolved reasoning level.
    pub display_level: ReasoningLevel,
    /// Exact native effort value, when selectable.
    pub native_effort: Option<String>,
    /// Required native collaboration mode, when applicable.
    pub collaboration_mode: Option<CollaborationMode>,
    /// Requested service tier, when explicitly selected.
    pub service_tier: Option<String>,
    /// Capability flags required by this preset.
    pub required_features: Vec<String>,
    /// Whether native interactive launch can select the preset.
    pub interactive_supported: bool,
    /// Whether native `codex exec` can select the preset.
    pub exec_supported: bool,
    /// Capability provenance used for resolution.
    pub source: CapabilitySource,
    /// Safe lower preset used when an automatic recommendation is unavailable.
    pub fallback: Option<Box<RoutePreset>>,
}

/// Explicit record of an allowed capability downgrade.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct Downgrade {
    /// Exact requested preset description.
    pub requested: String,
    /// Exact selected fallback description.
    pub selected: String,
    /// Capability reason for the downgrade.
    pub reason: String,
}

/// Fully materialized native process plan with exact argument boundaries.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LaunchPlan {
    /// Resolved native Codex executable.
    pub codex_binary: PathBuf,
    /// Repository working directory passed to Codex.
    pub working_directory: PathBuf,
    /// Interactive or exec launch surface.
    pub mode: LaunchMode,
    /// Catalog-resolved route preset.
    pub preset: RoutePreset,
    /// User-supplied arguments preserved in original order.
    pub inherited_args: Vec<OsString>,
    /// Minimal supported arguments injected by cauto.
    pub injected_args: Vec<OsString>,
    /// Original task argument retained as an operating-system string.
    pub prompt: Option<OsString>,
    /// Optional transparent downgrade record.
    pub downgrade: Option<Downgrade>,
}
