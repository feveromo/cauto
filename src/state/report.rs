use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::Path;

use serde::Serialize;

use crate::error::AppError;

use super::decision_log::DecisionRecord;
use super::tuning::{CalibrationStore, RepositoryTuning, analyze_repository, load_store};

#[derive(Clone, Debug, Default, Serialize)]
pub struct HistoryReport {
    pub schema_version: u32,
    pub total_decisions: u64,
    /// All successfully launched decisions, including adaptive agent turns.
    pub total_launched_decisions: u64,
    pub total_agent_decisions: u64,
    pub total_preview_decisions: u64,
    pub total_legacy_decisions: u64,
    /// Route distribution for all successfully launched decisions.
    pub route_distribution: BTreeMap<String, u64>,
    pub agent_route_distribution: BTreeMap<String, u64>,
    pub preview_route_distribution: BTreeMap<String, u64>,
    pub legacy_route_distribution: BTreeMap<String, u64>,
    pub model_family_distribution: BTreeMap<String, u64>,
    pub effort_distribution: BTreeMap<String, u64>,
    pub unresolved_generic_baseline_decisions: u64,
    pub unresolved_generic_baseline_rate_basis_points: u16,
    pub average_confidence_basis_points: u16,
    pub classifier_invocation_rate_basis_points: u16,
    pub classifier_failure_rate_basis_points: u16,
    pub catalog_fallback_rate_basis_points: u16,
    pub downgrade_rate_basis_points: u16,
    pub feedback_distribution: BTreeMap<String, u64>,
    pub feedback_source_distribution: BTreeMap<String, u64>,
    pub feedback_by_route: BTreeMap<String, BTreeMap<String, u64>>,
    pub feedback_by_repository: Vec<RepositoryTuning>,
    pub rules_most_often_raising_effort: Vec<(String, u64)>,
    pub rules_most_often_lowering_effort: Vec<(String, u64)>,
}

fn rate(numerator: u64, denominator: u64) -> u16 {
    (numerator * 10_000 + denominator / 2)
        .checked_div(denominator)
        .unwrap_or(0)
        .min(10_000) as u16
}

fn increment(map: &mut BTreeMap<String, u64>, key: impl Into<String>) {
    *map.entry(key.into()).or_default() += 1;
}

fn top_rules(map: BTreeMap<String, u64>) -> Vec<(String, u64)> {
    let mut values: Vec<_> = map.into_iter().collect();
    values.sort_by(|left, right| right.1.cmp(&left.1).then_with(|| left.0.cmp(&right.0)));
    values.truncate(10);
    values
}

pub fn build_report(path: &Path) -> Result<HistoryReport, AppError> {
    build_report_inner(path, CalibrationStore::default())
}

pub fn build_report_with_calibrations(
    path: &Path,
    calibration_path: &Path,
) -> Result<HistoryReport, AppError> {
    let store = load_store(calibration_path).unwrap_or_default();
    build_report_inner(path, store)
}

fn build_report_inner(path: &Path, store: CalibrationStore) -> Result<HistoryReport, AppError> {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let mut report = HistoryReport {
                schema_version: 3,
                ..HistoryReport::default()
            };
            report.feedback_by_repository = analyze_repository(path, &store, None)?.repositories;
            return Ok(report);
        }
        Err(source) => {
            return Err(AppError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let mut report = HistoryReport {
        schema_version: 3,
        ..HistoryReport::default()
    };
    let mut confidence_total = 0_u64;
    let mut classifier_runs = 0_u64;
    let mut classifier_failures = 0_u64;
    let mut fallbacks = 0_u64;
    let mut downgrades = 0_u64;
    let mut raising = BTreeMap::new();
    let mut lowering = BTreeMap::new();
    let mut decision_routes = BTreeMap::new();
    for line in std::io::BufReader::new(file).lines() {
        let line = line.map_err(|source| AppError::Io {
            path: path.to_path_buf(),
            source,
        })?;
        let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) else {
            continue;
        };
        match value.get("record_type").and_then(|value| value.as_str()) {
            Some("decision") => {
                let mode = value
                    .get("decision_mode")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned);
                let Ok(record) = serde_json::from_value::<DecisionRecord>(value) else {
                    continue;
                };
                report.total_decisions += 1;
                let route = format!("{}:{}", record.selected_family, record.selected_effort);
                decision_routes.insert(record.decision_id.clone(), route.clone());
                if mode.as_deref() == Some("preview") {
                    report.total_preview_decisions += 1;
                    increment(&mut report.preview_route_distribution, route);
                    continue;
                }
                if !matches!(mode.as_deref(), Some("launched" | "agent")) {
                    // Records created before decision_mode existed cannot distinguish real
                    // launches from the previews that older cauto versions also persisted.
                    report.total_legacy_decisions += 1;
                    increment(&mut report.legacy_route_distribution, route);
                    continue;
                }
                report.total_launched_decisions += 1;
                if mode.as_deref() == Some("agent") {
                    report.total_agent_decisions += 1;
                    increment(&mut report.agent_route_distribution, route.clone());
                }
                increment(&mut report.route_distribution, route);
                increment(
                    &mut report.model_family_distribution,
                    record.selected_family.to_string(),
                );
                increment(
                    &mut report.effort_distribution,
                    record.selected_effort.to_string(),
                );
                report.unresolved_generic_baseline_decisions += u64::from(
                    record.matched_rule_ids.is_empty()
                        && record.dimensions == crate::routing::DimensionScores::default()
                        && (!record.classifier_ran || record.classifier_outcome != "success"),
                );
                confidence_total += u64::from(record.confidence_basis_points);
                classifier_runs += u64::from(record.classifier_ran);
                classifier_failures +=
                    u64::from(record.classifier_ran && record.classifier_outcome != "success");
                fallbacks += u64::from(matches!(
                    record.catalog_source,
                    crate::routing::CapabilitySource::Fallback
                ));
                downgrades += u64::from(record.downgrade.is_some());
                for rule in &record.raising_rule_ids {
                    increment(&mut raising, rule.clone());
                }
                for rule in &record.lowering_rule_ids {
                    increment(&mut lowering, rule.clone());
                }
            }
            Some("feedback") => {
                if let Some(feedback) = value.get("feedback").and_then(|value| value.as_str()) {
                    increment(&mut report.feedback_distribution, feedback);
                    let source = value
                        .get("source")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or("legacy-unspecified");
                    increment(&mut report.feedback_source_distribution, source);
                    if let Some(route) = value
                        .get("decision_id")
                        .and_then(|value| value.as_str())
                        .and_then(|decision_id| decision_routes.get(decision_id))
                    {
                        increment(
                            report.feedback_by_route.entry(route.clone()).or_default(),
                            feedback,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    report.average_confidence_basis_points = confidence_total
        .checked_div(report.total_launched_decisions)
        .unwrap_or(0)
        .min(10_000) as u16;
    report.classifier_invocation_rate_basis_points =
        rate(classifier_runs, report.total_launched_decisions);
    report.classifier_failure_rate_basis_points = rate(classifier_failures, classifier_runs);
    report.catalog_fallback_rate_basis_points = rate(fallbacks, report.total_launched_decisions);
    report.downgrade_rate_basis_points = rate(downgrades, report.total_launched_decisions);
    report.unresolved_generic_baseline_rate_basis_points = rate(
        report.unresolved_generic_baseline_decisions,
        report.total_launched_decisions,
    );
    report.rules_most_often_raising_effort = top_rules(raising);
    report.rules_most_often_lowering_effort = top_rules(lowering);
    report.feedback_by_repository = analyze_repository(path, &store, None)?.repositories;
    Ok(report)
}
