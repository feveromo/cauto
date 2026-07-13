//! Redacted decision history, feedback, and aggregate reports.

pub mod decision_log;
pub mod feedback;
pub mod report;
pub mod tuning;

pub use decision_log::{
    DecisionRecord, append_decision, prompt_sha256, repository_identifier, sanitize_argv,
};
pub use feedback::{FeedbackKind, FeedbackSource, append_feedback, append_feedback_for_decision};
pub use report::{HistoryReport, build_report, build_report_with_calibrations};
pub use tuning::{
    AppliedCalibration, CalibrationStore, FeedbackCounts, RepositoryTuning, TuningAnalysis,
    analyze_repository, load_calibration, load_store, reset_repository, save_recommendation,
};
