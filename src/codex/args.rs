use std::ffi::{OsStr, OsString};
use std::str::FromStr;

use crate::error::AppError;
use crate::routing::ReasoningLevel;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ExplicitNativeOverrides {
    pub model: Option<String>,
    pub effort: Option<ReasoningLevel>,
    pub effort_raw: Option<String>,
    pub service_tier: Option<String>,
    pub profile: Option<String>,
}

fn utf8(value: &OsStr) -> Option<&str> {
    value.to_str()
}

fn scalar_text(raw: &str) -> String {
    let wrapped = format!("value = {raw}");
    if let Ok(table) = toml::from_str::<toml::Table>(&wrapped)
        && let Some(value) = table.get("value")
    {
        return match value {
            toml::Value::String(value) => value.clone(),
            other => other.to_string(),
        };
    }
    raw.to_owned()
}

fn inspect_config(assignment: &str, overrides: &mut ExplicitNativeOverrides) {
    let Some((key, raw_value)) = assignment.split_once('=') else {
        return;
    };
    let key = key.trim();
    let value = scalar_text(raw_value.trim());
    match key {
        "model" => overrides.model = Some(value),
        "model_reasoning_effort" => {
            overrides.effort_raw = Some(value.clone());
            overrides.effort = ReasoningLevel::from_str(&value).ok();
        }
        "service_tier" => overrides.service_tier = Some(value),
        _ => {}
    }
}

/// Inspects forwarded argv without changing, deleting, or reordering any value.
pub fn inspect_forwarded(args: &[OsString]) -> Result<ExplicitNativeOverrides, AppError> {
    let mut overrides = ExplicitNativeOverrides::default();
    let mut index = 0;
    while index < args.len() {
        let Some(argument) = utf8(&args[index]) else {
            index += 1;
            continue;
        };
        match argument {
            "--model" | "-m" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    AppError::InvalidArguments(format!("{argument} requires a value after --"))
                })?;
                if let Some(value) = utf8(value) {
                    overrides.model = Some(value.to_owned());
                }
                index += 2;
                continue;
            }
            "--profile" | "-p" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    AppError::InvalidArguments(format!("{argument} requires a value after --"))
                })?;
                if let Some(value) = utf8(value) {
                    overrides.profile = Some(value.to_owned());
                }
                index += 2;
                continue;
            }
            "--config" | "-c" => {
                let value = args.get(index + 1).ok_or_else(|| {
                    AppError::InvalidArguments(format!("{argument} requires key=value after --"))
                })?;
                if let Some(value) = utf8(value) {
                    inspect_config(value, &mut overrides);
                }
                index += 2;
                continue;
            }
            _ => {}
        }
        if let Some(value) = argument.strip_prefix("--model=") {
            overrides.model = Some(value.to_owned());
        } else if let Some(value) = argument.strip_prefix("--profile=") {
            overrides.profile = Some(value.to_owned());
        } else if let Some(value) = argument.strip_prefix("--config=") {
            inspect_config(value, &mut overrides);
        } else if argument.split_once('=').is_some_and(|(key, _)| {
            matches!(key, "model" | "model_reasoning_effort" | "service_tier")
        }) {
            inspect_config(argument, &mut overrides);
        }
        index += 1;
    }
    Ok(overrides)
}

#[must_use]
pub fn quoted_config(key: &str, value: &str) -> OsString {
    let encoded = toml::Value::String(value.to_owned()).to_string();
    OsString::from(format!("{key}={encoded}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_native_overrides_use_last_value() {
        let args = [
            "--model=gpt-5.6-terra",
            "-c",
            "model_reasoning_effort=\"medium\"",
            "--config=model_reasoning_effort='high'",
            "--config",
            "unrelated=\"model=max\"",
            "-m",
            "gpt-5.6-sol",
        ]
        .map(OsString::from);
        let found = inspect_forwarded(&args).unwrap();
        assert_eq!(found.model.as_deref(), Some("gpt-5.6-sol"));
        assert_eq!(found.effort, Some(ReasoningLevel::High));
    }
}
