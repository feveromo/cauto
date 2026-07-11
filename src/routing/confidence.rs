use super::Confidence;

/// Evidence-quality inputs used to calculate confidence independently of model strength.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct EvidenceQuality {
    /// Number of policy rules that matched.
    pub matched_rule_count: u16,
    /// Number of independent policy sources represented by the matches.
    pub independent_rule_sources: u8,
    /// Number of explicit task paths found.
    pub explicit_path_count: u16,
    /// Whether the task contains a clear reproduction.
    pub clear_reproduction: bool,
    /// Whether the task contains a clear completion condition.
    pub clear_completion: bool,
    /// Whether repository identity is known.
    pub known_repository: bool,
    /// Whether the task text is too vague for a strong assessment.
    pub vague_prompt: bool,
    /// Whether routing is relying on an emergency catalog.
    pub unknown_catalog: bool,
    /// Whether applicable AGENTS metadata was malformed or truncated.
    pub malformed_agents: bool,
    /// Whether dirty metadata weakens the available evidence.
    pub dirty_metadata: bool,
    /// Number of incompatible routing constraints.
    pub conflict_count: u16,
    /// Signed confidence adjustment supplied by matched rules, in basis points.
    pub rule_confidence_delta: i32,
}

/// Computes confidence in basis points using only integer arithmetic.
#[must_use]
pub fn confidence_for(evidence: EvidenceQuality) -> Confidence {
    let mut points = 5_400_i32;
    points += i32::from(evidence.matched_rule_count.min(4)) * 450;
    points += i32::from(evidence.independent_rule_sources.saturating_sub(1)) * 250;
    points += i32::from(evidence.explicit_path_count.min(3)) * 250;
    points += if evidence.clear_reproduction { 700 } else { 0 };
    points += if evidence.clear_completion { 600 } else { 0 };
    points += if evidence.known_repository { 350 } else { -450 };
    points -= if evidence.matched_rule_count == 0 {
        650
    } else {
        0
    };
    points -= if evidence.vague_prompt { 1_100 } else { 0 };
    points -= if evidence.unknown_catalog { 900 } else { 0 };
    points -= if evidence.malformed_agents { 450 } else { 0 };
    points -= if evidence.dirty_metadata { 150 } else { 0 };
    points -= i32::from(evidence.conflict_count) * 1_200;
    points += evidence.rule_confidence_delta;
    Confidence::from_basis_points(points.clamp(0, 10_000) as u16)
        .expect("clamped confidence is valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflicts_reduce_confidence() {
        let baseline = confidence_for(EvidenceQuality::default());
        let conflict = confidence_for(EvidenceQuality {
            conflict_count: 1,
            ..EvidenceQuality::default()
        });
        assert!(conflict < baseline);
    }
}
