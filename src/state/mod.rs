//! Redacted decision history, feedback, and aggregate reports.

pub mod decision_log;
pub mod feedback;
pub mod report;

pub use decision_log::{
    DecisionRecord, append_decision, prompt_sha256, repository_identifier, sanitize_argv,
};
pub use feedback::{FeedbackKind, append_feedback};
pub use report::{HistoryReport, build_report};
