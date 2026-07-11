use super::{DimensionScores, Reason};

/// Integer weights for the six complexity dimensions.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Weights {
    /// Relative scope weight.
    pub scope: u16,
    /// Relative ambiguity weight.
    pub ambiguity: u16,
    /// Relative cost-of-error weight.
    pub cost_of_being_wrong: u16,
    /// Relative runtime-dependence weight.
    pub runtime_dependence: u16,
    /// Relative architectural-depth weight.
    pub architectural_depth: u16,
    /// Relative verification-burden weight.
    pub verification_burden: u16,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            scope: 20,
            ambiguity: 20,
            cost_of_being_wrong: 20,
            runtime_dependence: 15,
            architectural_depth: 15,
            verification_burden: 10,
        }
    }
}

impl Weights {
    /// Returns the sum used as the normalization denominator.
    #[must_use]
    pub const fn total(self) -> u32 {
        self.scope as u32
            + self.ambiguity as u32
            + self.cost_of_being_wrong as u32
            + self.runtime_dependence as u32
            + self.architectural_depth as u32
            + self.verification_burden as u32
    }
}

/// Normalizes weighted 0..=4 dimensions to a deterministic 0..=100 score.
#[must_use]
pub fn normalized_score(dimensions: DimensionScores, weights: Weights) -> u8 {
    let total = weights.total();
    if total == 0 {
        return 0;
    }
    let weighted = u32::from(dimensions.scope.get()) * u32::from(weights.scope)
        + u32::from(dimensions.ambiguity.get()) * u32::from(weights.ambiguity)
        + u32::from(dimensions.cost_of_being_wrong.get()) * u32::from(weights.cost_of_being_wrong)
        + u32::from(dimensions.runtime_dependence.get()) * u32::from(weights.runtime_dependence)
        + u32::from(dimensions.architectural_depth.get()) * u32::from(weights.architectural_depth)
        + u32::from(dimensions.verification_burden.get()) * u32::from(weights.verification_burden);
    let denominator = 4 * total;
    ((weighted * 100 + denominator / 2) / denominator).min(100) as u8
}

#[must_use]
/// Builds explainable weighted contributions for nonzero dimensions.
pub fn dimension_reasons(dimensions: DimensionScores, weights: Weights) -> Vec<Reason> {
    let mut reasons = Vec::with_capacity(6);
    let values = [
        ("scope", dimensions.scope.get(), weights.scope),
        ("ambiguity", dimensions.ambiguity.get(), weights.ambiguity),
        (
            "cost of being wrong",
            dimensions.cost_of_being_wrong.get(),
            weights.cost_of_being_wrong,
        ),
        (
            "runtime dependence",
            dimensions.runtime_dependence.get(),
            weights.runtime_dependence,
        ),
        (
            "architectural depth",
            dimensions.architectural_depth.get(),
            weights.architectural_depth,
        ),
        (
            "verification burden",
            dimensions.verification_burden.get(),
            weights.verification_burden,
        ),
    ];
    for (label, value, weight) in values {
        if value > 0 {
            reasons.push(Reason {
                label: label.to_owned(),
                contribution: i16::from(value) * weight as i16,
            });
        }
    }
    reasons.sort_by_key(|reason| std::cmp::Reverse(reason.contribution));
    reasons
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::routing::BoundedScore;

    #[test]
    fn weighted_extremes_are_exact() {
        let zero = BoundedScore::new(0).unwrap();
        let four = BoundedScore::new(4).unwrap();
        let mut dimensions = DimensionScores {
            scope: zero,
            ambiguity: zero,
            cost_of_being_wrong: zero,
            runtime_dependence: zero,
            architectural_depth: zero,
            verification_burden: zero,
            ..DimensionScores::default()
        };
        assert_eq!(normalized_score(dimensions, Weights::default()), 0);
        dimensions.scope = four;
        dimensions.ambiguity = four;
        dimensions.cost_of_being_wrong = four;
        dimensions.runtime_dependence = four;
        dimensions.architectural_depth = four;
        dimensions.verification_burden = four;
        assert_eq!(normalized_score(dimensions, Weights::default()), 100);
    }
}
