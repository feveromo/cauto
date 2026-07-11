use std::collections::HashSet;

use super::schema::{RawConfig, RawWeights};

fn choose<T>(lower: Option<T>, higher: Option<T>) -> Option<T> {
    higher.or(lower)
}

impl RawWeights {
    #[must_use]
    pub fn merge(self, higher: Self) -> Self {
        Self {
            scope: choose(self.scope, higher.scope),
            ambiguity: choose(self.ambiguity, higher.ambiguity),
            cost_of_being_wrong: choose(self.cost_of_being_wrong, higher.cost_of_being_wrong),
            runtime_dependence: choose(self.runtime_dependence, higher.runtime_dependence),
            architectural_depth: choose(self.architectural_depth, higher.architectural_depth),
            verification_burden: choose(self.verification_burden, higher.verification_burden),
        }
    }
}

impl RawConfig {
    /// Merges a higher-precedence typed layer over this one.
    #[must_use]
    pub fn merge(self, mut higher: Self) -> Self {
        let higher_ids: HashSet<&str> = higher.rules.iter().map(|rule| rule.id.as_str()).collect();
        let mut rules = self.rules;
        rules.retain(|rule| !higher_ids.contains(rule.id.as_str()));
        rules.append(&mut higher.rules);
        Self {
            version: choose(self.version, higher.version),
            classifier: choose(self.classifier, higher.classifier),
            classifier_confidence_threshold: choose(
                self.classifier_confidence_threshold,
                higher.classifier_confidence_threshold,
            ),
            default_model: choose(self.default_model, higher.default_model),
            default_effort: choose(self.default_effort, higher.default_effort),
            fast_mode: choose(self.fast_mode, higher.fast_mode),
            ultra_requires_opt_in: choose(self.ultra_requires_opt_in, higher.ultra_requires_opt_in),
            allow_automatic_downgrade: choose(
                self.allow_automatic_downgrade,
                higher.allow_automatic_downgrade,
            ),
            log_raw_prompts: choose(self.log_raw_prompts, higher.log_raw_prompts),
            strict_logging: choose(self.strict_logging, higher.strict_logging),
            catalog_cache_hours: choose(self.catalog_cache_hours, higher.catalog_cache_hours),
            git_timeout_ms: choose(self.git_timeout_ms, higher.git_timeout_ms),
            catalog_timeout_ms: choose(self.catalog_timeout_ms, higher.catalog_timeout_ms),
            classifier_timeout_seconds: choose(
                self.classifier_timeout_seconds,
                higher.classifier_timeout_seconds,
            ),
            hysteresis_points: choose(self.hysteresis_points, higher.hysteresis_points),
            weights: self.weights.merge(higher.weights),
            rules,
        }
    }
}
