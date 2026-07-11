use std::collections::HashSet;
use std::num::NonZeroU64;
use std::path::Path;
use std::str::FromStr;

use globset::Glob;

use super::schema::{RawConfig, RawDimensionDeltas, RawRule, ValidatedConfig, ValidatedRule};
use crate::config::schema::TimeoutMillis;
use crate::error::AppError;
use crate::routing::{ClassifierMode, FastMode, ModelFamily, ReasoningLevel, Weights};

fn invalid(
    file: &Path,
    toml_path: impl Into<String>,
    value: impl Into<String>,
    expected: impl Into<String>,
    suggestion: impl Into<String>,
) -> AppError {
    AppError::ConfigValidation {
        path: file.to_path_buf(),
        toml_path: toml_path.into(),
        value: value.into(),
        expected: expected.into(),
        suggestion: suggestion.into(),
    }
}

pub fn validate_layer(raw: &RawConfig, file: &Path) -> Result<(), AppError> {
    if let Some(version) = raw.version
        && version != 1
    {
        return Err(invalid(
            file,
            "version",
            version.to_string(),
            "1",
            "Set version = 1.",
        ));
    }
    if let Some(value) = &raw.classifier {
        ClassifierMode::from_str(value).map_err(|_| {
            invalid(
                file,
                "classifier",
                value,
                "auto, always, or never",
                "Use classifier = \"auto\" for the default behavior.",
            )
        })?;
    }
    if let Some(value) = raw.classifier_confidence_threshold
        && (!value.is_finite() || !(0.0..=1.0).contains(&value))
    {
        return Err(invalid(
            file,
            "classifier_confidence_threshold",
            value.to_string(),
            "a finite number from 0.0 through 1.0",
            "Use 0.72 for the default threshold.",
        ));
    }
    if raw.log_raw_prompts == Some(true) {
        return Err(invalid(
            file,
            "log_raw_prompts",
            "true",
            "false",
            "cauto never stores raw prompts; use prompt hashes for correlation.",
        ));
    }
    validate_optional_range(
        file,
        "catalog_cache_hours",
        raw.catalog_cache_hours,
        1,
        8_760,
    )?;
    validate_optional_range(file, "git_timeout_ms", raw.git_timeout_ms, 1, 10_000)?;
    validate_optional_range(
        file,
        "catalog_timeout_ms",
        raw.catalog_timeout_ms,
        100,
        60_000,
    )?;
    validate_optional_range(
        file,
        "classifier_timeout_seconds",
        raw.classifier_timeout_seconds,
        1,
        600,
    )?;
    if let Some(points) = raw.hysteresis_points
        && points > 20
    {
        return Err(invalid(
            file,
            "hysteresis_points",
            points.to_string(),
            "0 through 20",
            "Use 2 for the default threshold band.",
        ));
    }
    for (name, value) in [
        ("scope", raw.weights.scope),
        ("ambiguity", raw.weights.ambiguity),
        ("cost_of_being_wrong", raw.weights.cost_of_being_wrong),
        ("runtime_dependence", raw.weights.runtime_dependence),
        ("architectural_depth", raw.weights.architectural_depth),
        ("verification_burden", raw.weights.verification_burden),
    ] {
        if value.is_some_and(|weight| weight > 100) {
            return Err(invalid(
                file,
                format!("weights.{name}"),
                value.expect("checked option").to_string(),
                "0 through 100",
                "Keep each weight at or below 100.",
            ));
        }
    }
    let mut ids = HashSet::with_capacity(raw.rules.len());
    for (index, rule) in raw.rules.iter().enumerate() {
        validate_rule(rule, index, file)?;
        if !ids.insert(rule.id.as_str()) {
            return Err(invalid(
                file,
                format!("rules[{index}].id"),
                &rule.id,
                "a unique rule id",
                "Rename or remove the duplicate rule.",
            ));
        }
    }
    Ok(())
}

fn validate_optional_range(
    file: &Path,
    field: &str,
    value: Option<u64>,
    min: u64,
    max: u64,
) -> Result<(), AppError> {
    if let Some(value) = value
        && !(min..=max).contains(&value)
    {
        return Err(invalid(
            file,
            field,
            value.to_string(),
            format!("{min} through {max}"),
            format!("Choose a value inside {min}..={max}."),
        ));
    }
    Ok(())
}

fn validate_rule(rule: &RawRule, index: usize, file: &Path) -> Result<(), AppError> {
    let base = format!("rules[{index}]");
    if rule.id.trim().is_empty() || rule.id.len() > 96 {
        return Err(invalid(
            file,
            format!("{base}.id"),
            &rule.id,
            "a non-empty id of at most 96 bytes",
            "Use a short kebab-case identifier.",
        ));
    }
    if rule.phrases.is_empty() && rule.path_globs.is_empty() {
        return Err(invalid(
            file,
            &base,
            &rule.id,
            "at least one phrase or path_glob",
            "Add bounded routing evidence to the rule.",
        ));
    }
    for phrase in &rule.phrases {
        if phrase.trim().is_empty() || phrase.len() > 512 {
            return Err(invalid(
                file,
                format!("{base}.phrases"),
                phrase,
                "non-empty phrases of at most 512 bytes",
                "Remove the empty phrase or shorten it.",
            ));
        }
    }
    for path_glob in &rule.path_globs {
        Glob::new(path_glob).map_err(|error| {
            invalid(
                file,
                format!("{base}.path_globs"),
                path_glob,
                "a valid repository-relative glob",
                format!("Correct the glob syntax: {error}"),
            )
        })?;
    }
    validate_deltas(file, &base, rule.dimension_deltas)?;
    if !rule.confidence_delta.is_finite() || !(-1.0..=1.0).contains(&rule.confidence_delta) {
        return Err(invalid(
            file,
            format!("{base}.confidence_delta"),
            rule.confidence_delta.to_string(),
            "a finite number from -1.0 through 1.0",
            "Use a small adjustment such as 0.10.",
        ));
    }
    let floor = parse_optional_family(file, &base, "family_floor", rule.family_floor.as_deref())?;
    let ceiling = parse_optional_family(
        file,
        &base,
        "family_ceiling",
        rule.family_ceiling.as_deref(),
    )?;
    if let (Some(floor), Some(ceiling)) = (&floor, &ceiling)
        && floor.rank() > ceiling.rank()
    {
        return Err(invalid(
            file,
            &base,
            format!("family floor {floor}, ceiling {ceiling}"),
            "a floor no stronger than the ceiling",
            "Remove one bound or widen the ceiling.",
        ));
    }
    let effort_floor =
        parse_optional_effort(file, &base, "effort_floor", rule.effort_floor.as_deref())?;
    let effort_ceiling = parse_optional_effort(
        file,
        &base,
        "effort_ceiling",
        rule.effort_ceiling.as_deref(),
    )?;
    if let (Some(floor), Some(ceiling)) = (effort_floor, effort_ceiling)
        && floor > ceiling
    {
        return Err(invalid(
            file,
            &base,
            format!(
                "effort floor {}, ceiling {}",
                floor.display_name(),
                ceiling.display_name()
            ),
            "a floor no stronger than the ceiling",
            "Remove one bound or widen the ceiling.",
        ));
    }
    Ok(())
}

fn validate_deltas(file: &Path, base: &str, deltas: RawDimensionDeltas) -> Result<(), AppError> {
    for (name, value) in [
        ("scope", deltas.scope),
        ("ambiguity", deltas.ambiguity),
        ("cost_of_being_wrong", deltas.cost_of_being_wrong),
        ("runtime_dependence", deltas.runtime_dependence),
        ("architectural_depth", deltas.architectural_depth),
        ("verification_burden", deltas.verification_burden),
        ("parallelizability", deltas.parallelizability),
    ] {
        if !(-4..=4).contains(&value) {
            return Err(invalid(
                file,
                format!("{base}.dimension_deltas.{name}"),
                value.to_string(),
                "-4 through 4",
                "Use a bounded dimension adjustment.",
            ));
        }
    }
    Ok(())
}

fn parse_optional_family(
    file: &Path,
    base: &str,
    field: &str,
    value: Option<&str>,
) -> Result<Option<ModelFamily>, AppError> {
    value
        .map(|value| {
            ModelFamily::from_str(value).map_err(|_| {
                invalid(
                    file,
                    format!("{base}.{field}"),
                    value,
                    "luna, terra, or sol",
                    "Use a known model family name.",
                )
            })
        })
        .transpose()
}

fn parse_optional_effort(
    file: &Path,
    base: &str,
    field: &str,
    value: Option<&str>,
) -> Result<Option<ReasoningLevel>, AppError> {
    value
        .map(|value| {
            ReasoningLevel::from_str(value).map_err(|_| {
                invalid(
                    file,
                    format!("{base}.{field}"),
                    value,
                    "minimal, low, medium, high, xhigh, max, or ultra",
                    "Use xhigh for Extra High; Max and Ultra remain capability-gated.",
                )
            })
        })
        .transpose()
}

pub fn into_validated(raw: RawConfig, file: &Path) -> Result<ValidatedConfig, AppError> {
    validate_layer(&raw, file)?;
    let defaults = ValidatedConfig::default();
    let classifier = raw
        .classifier
        .as_deref()
        .map(ClassifierMode::from_str)
        .transpose()
        .map_err(|message| invalid(file, "classifier", message, "a valid mode", "Use auto."))?
        .unwrap_or(defaults.classifier);
    let threshold = raw.classifier_confidence_threshold.unwrap_or(0.72);
    let default_effort = raw
        .default_effort
        .as_deref()
        .map(ReasoningLevel::from_str)
        .transpose()
        .map_err(|message| {
            invalid(
                file,
                "default_effort",
                message,
                "a valid effort",
                "Use medium.",
            )
        })?
        .unwrap_or(defaults.default_effort);
    let fast_mode = match raw.fast_mode.as_deref().unwrap_or("inherit") {
        "fast" | "on" => FastMode::Fast,
        "no-fast" | "off" => FastMode::NoFast,
        "inherit" => FastMode::Inherit,
        value => {
            return Err(invalid(
                file,
                "fast_mode",
                value,
                "fast, no-fast, or inherit",
                "Use inherit to preserve native Codex behavior.",
            ));
        }
    };
    let weights = Weights {
        scope: raw.weights.scope.unwrap_or(defaults.weights.scope),
        ambiguity: raw.weights.ambiguity.unwrap_or(defaults.weights.ambiguity),
        cost_of_being_wrong: raw
            .weights
            .cost_of_being_wrong
            .unwrap_or(defaults.weights.cost_of_being_wrong),
        runtime_dependence: raw
            .weights
            .runtime_dependence
            .unwrap_or(defaults.weights.runtime_dependence),
        architectural_depth: raw
            .weights
            .architectural_depth
            .unwrap_or(defaults.weights.architectural_depth),
        verification_burden: raw
            .weights
            .verification_burden
            .unwrap_or(defaults.weights.verification_burden),
    };
    if weights.total() == 0 {
        return Err(invalid(
            file,
            "weights",
            "all zero",
            "at least one positive weight",
            "Restore the default weights.",
        ));
    }
    let mut rules = Vec::with_capacity(raw.rules.len());
    for rule in raw.rules {
        rules.push(ValidatedRule {
            id: rule.id,
            description: rule.description,
            phrases: rule.phrases,
            path_globs: rule.path_globs,
            dimension_deltas: rule.dimension_deltas.into(),
            family_floor: parse_optional_family(
                file,
                "rule",
                "family_floor",
                rule.family_floor.as_deref(),
            )?,
            family_ceiling: parse_optional_family(
                file,
                "rule",
                "family_ceiling",
                rule.family_ceiling.as_deref(),
            )?,
            effort_floor: parse_optional_effort(
                file,
                "rule",
                "effort_floor",
                rule.effort_floor.as_deref(),
            )?,
            effort_ceiling: parse_optional_effort(
                file,
                "rule",
                "effort_ceiling",
                rule.effort_ceiling.as_deref(),
            )?,
            confidence_delta_basis_points: (rule.confidence_delta * 10_000.0).round() as i16,
            reason: rule.reason,
            source: rule.source.unwrap_or(crate::routing::RuleSource::Project),
        });
    }
    Ok(ValidatedConfig {
        classifier,
        classifier_confidence_threshold_basis_points: (threshold * 10_000.0).round() as u16,
        default_model: raw.default_model.unwrap_or(defaults.default_model),
        default_effort,
        fast_mode,
        ultra_requires_opt_in: raw.ultra_requires_opt_in.unwrap_or(true),
        allow_automatic_downgrade: raw.allow_automatic_downgrade.unwrap_or(true),
        strict_logging: raw.strict_logging.unwrap_or(false),
        catalog_cache_hours: NonZeroU64::new(raw.catalog_cache_hours.unwrap_or(12))
            .expect("validated non-zero hours"),
        git_timeout: TimeoutMillis::new(raw.git_timeout_ms.unwrap_or(250))
            .expect("validated non-zero timeout"),
        catalog_timeout: TimeoutMillis::new(raw.catalog_timeout_ms.unwrap_or(2_500))
            .expect("validated non-zero timeout"),
        classifier_timeout: TimeoutMillis::new(
            raw.classifier_timeout_seconds.unwrap_or(45) * 1_000,
        )
        .expect("validated non-zero timeout"),
        hysteresis_points: raw.hysteresis_points.unwrap_or(2),
        weights,
        rules,
    })
}
