use crate::routing::{Downgrade, RouteDecision, RoutePreset};

pub fn render(
    decision: &RouteDecision,
    preset: &RoutePreset,
    downgrade: Option<&Downgrade>,
    verbose: bool,
) -> String {
    let mut output = String::with_capacity(512);
    output.push_str(&format!(
        "Selected: {} / {}\nScore: {}/100\nConfidence: {}%\n",
        preset.model_id,
        preset.display_level.display_name(),
        decision.normalized_score,
        (u32::from(decision.confidence.basis_points()) + 50) / 100
    ));
    let why = if decision.task_type == crate::routing::TaskType::Empty {
        vec!["no task supplied; using configured or explicit session route"]
    } else {
        crate::routing::explain::compact_reasons(decision, 3)
    };
    if !why.is_empty() {
        output.push_str("Why: ");
        output.push_str(&why.join(", "));
        output.push('\n');
    }
    if decision.ultra_selected {
        output.push_str("Ultra: selected with explicit delegation authorization\n");
    } else if decision.ultra_candidate {
        output.push_str(
            "Ultra: candidate, not selected; explicit subagent authorization is required\n",
        );
    } else {
        output.push_str("Ultra: not eligible; task lacks high, independent parallel tracks\n");
    }
    if let Some(downgrade) = downgrade {
        output.push_str(&format!(
            "Downgrade: {} -> {} ({})\n",
            downgrade.requested, downgrade.selected, downgrade.reason
        ));
    }
    if verbose {
        output.push_str("\nEvidence:\n");
        for matched in &decision.matched_rules {
            output.push_str(&format!(
                "  {}: {} ({})\n",
                matched.rule_id, matched.reason, matched.matched_text_or_path
            ));
        }
        for conflict in &decision.conflicts {
            output.push_str(&format!("  conflict: {}\n", conflict.message));
        }
    }
    output
}
