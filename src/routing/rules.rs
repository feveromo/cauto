use aho_corasick::{AhoCorasick, AhoCorasickBuilder};
use globset::{Glob, GlobSet, GlobSetBuilder};

use crate::config::ValidatedRule;
use crate::error::AppError;

use super::{
    Conflict, DimensionScores, ModelFamily, ReasoningLevel, RuleMatch, SelectionConstraints,
};

/// All invocation-local rule matchers, compiled once.
#[derive(Debug)]
pub struct CompiledRules {
    rules: Vec<ValidatedRule>,
    phrase_matcher: Option<AhoCorasick>,
    phrase_to_rule: Vec<usize>,
    path_matcher: Option<GlobSet>,
    path_to_rule: Vec<usize>,
}

/// The aggregate result of applying every matched policy rule once.
#[derive(Clone, Debug)]
pub struct RuleApplication {
    /// Dimension scores after signed rule deltas.
    pub dimensions: DimensionScores,
    /// Strongest compatible floors and ceilings found.
    pub constraints: SelectionConstraints,
    /// Structured evidence for every matched rule.
    pub matches: Vec<RuleMatch>,
    /// Contradictory bounds detected during application.
    pub conflicts: Vec<Conflict>,
    /// Total signed confidence adjustment, in basis points.
    pub confidence_delta_basis_points: i32,
}

impl CompiledRules {
    /// Compiles phrase and path matchers for one invocation.
    pub fn new(rules: Vec<ValidatedRule>) -> Result<Self, AppError> {
        let phrase_count: usize = rules.iter().map(|rule| rule.phrases.len()).sum();
        let mut phrases = Vec::with_capacity(phrase_count);
        let mut phrase_to_rule = Vec::with_capacity(phrase_count);
        let path_count: usize = rules.iter().map(|rule| rule.path_globs.len()).sum();
        let mut path_builder = GlobSetBuilder::new();
        let mut path_to_rule = Vec::with_capacity(path_count);
        for (rule_index, rule) in rules.iter().enumerate() {
            for phrase in &rule.phrases {
                phrases.push(phrase.as_str());
                phrase_to_rule.push(rule_index);
            }
            for path_glob in &rule.path_globs {
                let compiled = Glob::new(path_glob).map_err(|error| AppError::ConfigParse {
                    path: std::path::PathBuf::from(".cauto.toml"),
                    message: format!("invalid glob {path_glob:?}: {error}"),
                })?;
                path_builder.add(compiled);
                path_to_rule.push(rule_index);
            }
        }
        let phrase_matcher = if phrases.is_empty() {
            None
        } else {
            Some(
                AhoCorasickBuilder::new()
                    .ascii_case_insensitive(true)
                    .build(phrases)
                    .map_err(|error| AppError::ConfigParse {
                        path: std::path::PathBuf::from(".cauto.toml"),
                        message: format!("failed to compile phrase rules: {error}"),
                    })?,
            )
        };
        let path_matcher = if path_to_rule.is_empty() {
            None
        } else {
            Some(
                path_builder
                    .build()
                    .map_err(|error| AppError::ConfigParse {
                        path: std::path::PathBuf::from(".cauto.toml"),
                        message: format!("failed to compile path rules: {error}"),
                    })?,
            )
        };
        Ok(Self {
            rules,
            phrase_matcher,
            phrase_to_rule,
            path_matcher,
            path_to_rule,
        })
    }

    /// Matches each rule at most once, retaining one concise evidence string.
    #[must_use]
    pub fn evaluate(
        &self,
        normalized_prompt: &str,
        explicit_paths: &[String],
        base_dimensions: DimensionScores,
    ) -> RuleApplication {
        let mut evidence: Vec<Option<String>> = vec![None; self.rules.len()];
        if let Some(matcher) = &self.phrase_matcher {
            for matched in matcher.find_iter(normalized_prompt) {
                let pattern = matched.pattern().as_usize();
                let rule_index = self.phrase_to_rule[pattern];
                if evidence[rule_index].is_none() {
                    evidence[rule_index] =
                        Some(normalized_prompt[matched.start()..matched.end()].to_owned());
                }
            }
        }
        if let Some(matcher) = &self.path_matcher {
            for path in explicit_paths {
                for pattern in matcher.matches(path) {
                    let rule_index = self.path_to_rule[pattern];
                    if evidence[rule_index].is_none() {
                        evidence[rule_index] = Some(path.clone());
                    }
                }
            }
        }

        let mut dimensions = base_dimensions;
        let mut matches = Vec::with_capacity(self.rules.len().min(8));
        let mut family_floor: Option<ModelFamily> = None;
        let mut family_ceiling: Option<ModelFamily> = None;
        let mut effort_floor: Option<ReasoningLevel> = None;
        let mut effort_ceiling: Option<ReasoningLevel> = None;
        let mut confidence_delta_basis_points = 0_i32;
        for (rule, matched) in self.rules.iter().zip(evidence) {
            let Some(matched_text_or_path) = matched else {
                continue;
            };
            dimensions.apply(rule.dimension_deltas);
            if let Some(floor) = &rule.family_floor
                && family_floor
                    .as_ref()
                    .is_none_or(|current| floor.rank() > current.rank())
            {
                family_floor = Some(floor.clone());
            }
            if let Some(ceiling) = &rule.family_ceiling
                && family_ceiling
                    .as_ref()
                    .is_none_or(|current| ceiling.rank() < current.rank())
            {
                family_ceiling = Some(ceiling.clone());
            }
            if let Some(floor) = rule.effort_floor
                && effort_floor.is_none_or(|current| floor > current)
            {
                effort_floor = Some(floor);
            }
            if let Some(ceiling) = rule.effort_ceiling
                && effort_ceiling.is_none_or(|current| ceiling < current)
            {
                effort_ceiling = Some(ceiling);
            }
            confidence_delta_basis_points += i32::from(rule.confidence_delta_basis_points);
            matches.push(RuleMatch {
                rule_id: rule.id.clone(),
                source: rule.source.clone(),
                matched_text_or_path,
                dimension_effects: rule.dimension_deltas,
                family_floor: rule.family_floor.clone(),
                family_ceiling: rule.family_ceiling.clone(),
                effort_floor: rule.effort_floor,
                effort_ceiling: rule.effort_ceiling,
                confidence_effect_basis_points: rule.confidence_delta_basis_points,
                reason: rule.reason.clone(),
            });
        }
        let mut conflicts = Vec::new();
        if let (Some(floor), Some(ceiling)) = (&family_floor, &family_ceiling)
            && floor.rank() > ceiling.rank()
        {
            conflicts.push(Conflict {
                kind: "matched-family-bounds".into(),
                message: format!(
                    "matched rules require family floor {floor} above ceiling {ceiling}"
                ),
            });
        }
        if let (Some(floor), Some(ceiling)) = (effort_floor, effort_ceiling)
            && floor > ceiling
        {
            conflicts.push(Conflict {
                kind: "matched-effort-bounds".into(),
                message: format!(
                    "matched rules require effort floor {} above ceiling {}",
                    floor.display_name(),
                    ceiling.display_name()
                ),
            });
        }
        RuleApplication {
            dimensions,
            constraints: SelectionConstraints {
                family_floor,
                family_ceiling,
                effort_floor,
                effort_ceiling,
                ..SelectionConstraints::default()
            },
            matches,
            conflicts,
            confidence_delta_basis_points,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::{DimensionDeltas, RuleSource};

    fn rule() -> ValidatedRule {
        ValidatedRule {
            id: "live".into(),
            description: String::new(),
            phrases: vec!["live client".into()],
            path_globs: vec!["src/**/*.rs".into()],
            dimension_deltas: DimensionDeltas {
                runtime_dependence: 2,
                ..DimensionDeltas::default()
            },
            family_floor: Some(ModelFamily::Sol),
            family_ceiling: None,
            effort_floor: Some(ReasoningLevel::High),
            effort_ceiling: None,
            confidence_delta_basis_points: 1_000,
            reason: "runtime proof".into(),
            source: RuleSource::Project,
        }
    }

    #[test]
    fn phrase_and_path_match_apply_rule_only_once() {
        let compiled = CompiledRules::new(vec![rule()]).unwrap();
        let result = compiled.evaluate(
            "fix live client src/main.rs",
            &["src/main.rs".into()],
            DimensionScores::default(),
        );
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.dimensions.runtime_dependence.get(), 2);
    }
}
