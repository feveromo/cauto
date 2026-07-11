use super::{Reason, RouteDecision};

/// Returns the highest-signal reason labels for compact human output.
#[must_use]
pub fn compact_reasons(decision: &RouteDecision, limit: usize) -> Vec<&str> {
    let mut reasons: Vec<&Reason> = decision.reasons.iter().collect();
    reasons.sort_by_key(|reason| std::cmp::Reverse(reason.contribution.abs()));
    reasons
        .into_iter()
        .map(|reason| reason.label.as_str())
        .filter(|label| !label.is_empty())
        .take(limit)
        .collect()
}
