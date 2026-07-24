use crate::routing::{Downgrade, RouteDecision, RoutePreset, TaskType};

fn task_label(task_type: &TaskType) -> &'static str {
    match task_type {
        TaskType::Empty => "empty/default",
        TaskType::Documentation => "documentation",
        TaskType::Mechanical => "mechanical",
        TaskType::Coding => "coding",
        TaskType::Diagnosis => "diagnosis",
        TaskType::Architecture => "architecture",
        TaskType::Research => "research",
        TaskType::Review => "review",
    }
}

pub fn render(
    decision: &RouteDecision,
    preset: &RoutePreset,
    downgrade: Option<&Downgrade>,
    verbose: bool,
) -> String {
    let mut output = String::with_capacity(768);
    output.push_str(&format!(
        "Selected: {} / {}\nTask: {}\nScore: {}/100\nConfidence: {}%\n",
        preset.model_id,
        preset.display_level.display_name(),
        task_label(&decision.task_type),
        decision.normalized_score,
        (u32::from(decision.confidence.basis_points()) + 50) / 100
    ));
    let why = if decision.task_type == TaskType::Empty {
        vec!["no task supplied; using configured or explicit session route"]
    } else {
        crate::routing::explain::compact_reasons(decision, 3)
    };
    if !why.is_empty() {
        output.push_str("Why: ");
        output.push_str(&why.join(", "));
        output.push('\n');
    }
    if let Some(calibration) = &decision.calibration {
        output.push_str(&format!(
            "Calibration: configured {:+}, applied {:+}; score {} -> {} ({})\n",
            calibration.configured_offset,
            calibration.applied_offset,
            calibration.base_score,
            calibration.calibrated_score,
            calibration.reason,
        ));
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
        let dimensions = decision.dimensions;
        output.push_str("\nDecision trace:\n");
        output.push_str(&format!(
            "  dimensions: scope={}, ambiguity={}, cost={}, runtime={}, architecture={}, verification={}, parallel={}\n",
            dimensions.scope.get(),
            dimensions.ambiguity.get(),
            dimensions.cost_of_being_wrong.get(),
            dimensions.runtime_dependence.get(),
            dimensions.architectural_depth.get(),
            dimensions.verification_burden.get(),
            dimensions.parallelizability.get(),
        ));
        if decision.matched_rules.is_empty() {
            output.push_str("  policy rules: none\n");
        } else {
            output.push_str("  policy rules:\n");
            for matched in &decision.matched_rules {
                output.push_str(&format!(
                    "    {}: {} ({})\n",
                    matched.rule_id, matched.reason, matched.matched_text_or_path
                ));
            }
        }
        for conflict in &decision.conflicts {
            output.push_str(&format!("  conflict: {}\n", conflict.message));
        }
    }
    output
}
