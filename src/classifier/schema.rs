use serde::Deserialize;

use crate::routing::{BoundedScore, Confidence, EscalationSignal, Reason};

const MAX_REASONS: usize = 12;
const MAX_SIGNALS: usize = 12;
const MAX_STRING_BYTES: usize = 512;

#[derive(Clone, Debug)]
pub struct ClassifierAssessment {
    pub task_type: String,
    pub scope: BoundedScore,
    pub ambiguity: BoundedScore,
    pub cost_of_being_wrong: BoundedScore,
    pub runtime_dependence: BoundedScore,
    pub architectural_depth: BoundedScore,
    pub verification_burden: BoundedScore,
    pub parallelizability: BoundedScore,
    pub confidence: Confidence,
    pub reasons: Vec<Reason>,
    pub escalation_signals: Vec<EscalationSignal>,
}

#[derive(Debug, Deserialize)]
struct RawClassifierAssessment {
    task_type: String,
    scope: u8,
    ambiguity: u8,
    cost_of_being_wrong: u8,
    runtime_dependence: u8,
    architectural_depth: u8,
    verification_burden: u8,
    parallelizability: u8,
    confidence: f64,
    #[serde(default)]
    reasons: Vec<String>,
    #[serde(default)]
    escalation_signals: Vec<String>,
}

impl ClassifierAssessment {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        let raw: RawClassifierAssessment =
            serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
        if raw.task_type.is_empty() || raw.task_type.len() > 128 {
            return Err("task_type must contain 1..=128 bytes".into());
        }
        if raw.reasons.len() > MAX_REASONS {
            return Err(format!("reasons exceeds the {MAX_REASONS}-item limit"));
        }
        if raw.escalation_signals.len() > MAX_SIGNALS {
            return Err(format!(
                "escalation_signals exceeds the {MAX_SIGNALS}-item limit"
            ));
        }
        if raw
            .reasons
            .iter()
            .chain(raw.escalation_signals.iter())
            .any(|value| value.is_empty() || value.len() > MAX_STRING_BYTES)
        {
            return Err(format!(
                "classifier strings must contain 1..={MAX_STRING_BYTES} bytes"
            ));
        }
        Ok(Self {
            task_type: raw.task_type,
            scope: BoundedScore::new(raw.scope)?,
            ambiguity: BoundedScore::new(raw.ambiguity)?,
            cost_of_being_wrong: BoundedScore::new(raw.cost_of_being_wrong)?,
            runtime_dependence: BoundedScore::new(raw.runtime_dependence)?,
            architectural_depth: BoundedScore::new(raw.architectural_depth)?,
            verification_burden: BoundedScore::new(raw.verification_burden)?,
            parallelizability: BoundedScore::new(raw.parallelizability)?,
            confidence: Confidence::from_ratio(raw.confidence)?,
            reasons: raw
                .reasons
                .into_iter()
                .map(|label| Reason {
                    label,
                    contribution: 0,
                })
                .collect(),
            escalation_signals: raw
                .escalation_signals
                .into_iter()
                .map(|label| EscalationSignal { label })
                .collect(),
        })
    }
}

#[must_use]
pub fn output_schema() -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "additionalProperties": false,
        "required": [
            "task_type", "scope", "ambiguity", "cost_of_being_wrong",
            "runtime_dependence", "architectural_depth", "verification_burden",
            "parallelizability", "confidence", "reasons", "escalation_signals"
        ],
        "properties": {
            "task_type": {"type": "string", "minLength": 1, "maxLength": 128},
            "scope": {"type": "integer", "minimum": 0, "maximum": 4},
            "ambiguity": {"type": "integer", "minimum": 0, "maximum": 4},
            "cost_of_being_wrong": {"type": "integer", "minimum": 0, "maximum": 4},
            "runtime_dependence": {"type": "integer", "minimum": 0, "maximum": 4},
            "architectural_depth": {"type": "integer", "minimum": 0, "maximum": 4},
            "verification_burden": {"type": "integer", "minimum": 0, "maximum": 4},
            "parallelizability": {"type": "integer", "minimum": 0, "maximum": 4},
            "confidence": {"type": "number", "minimum": 0.0, "maximum": 1.0},
            "reasons": {
                "type": "array", "maxItems": MAX_REASONS,
                "items": {"type": "string", "minLength": 1, "maxLength": MAX_STRING_BYTES}
            },
            "escalation_signals": {
                "type": "array", "maxItems": MAX_SIGNALS,
                "items": {"type": "string", "minLength": 1, "maxLength": MAX_STRING_BYTES}
            }
        }
    })
}
