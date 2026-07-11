use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::process::{Command, ExitCode, Stdio};

use crate::codex::args::quoted_config;
use crate::error::AppError;
use crate::routing::{LaunchMode, LaunchPlan};

#[derive(Clone, Copy, Debug, Default)]
pub struct InjectionPolicy {
    pub inject_model: bool,
    pub inject_effort: bool,
    pub inject_service_tier: bool,
}

pub fn materialize_args(plan: &LaunchPlan, policy: InjectionPolicy) -> Vec<OsString> {
    let capacity = plan.inherited_args.len() + plan.injected_args.len() + 10;
    let mut args = Vec::with_capacity(capacity);
    if plan.mode == LaunchMode::Exec {
        args.push(OsString::from("exec"));
    }
    args.push(OsString::from("--cd"));
    args.push(plan.working_directory.as_os_str().to_owned());
    if policy.inject_model {
        args.push(OsString::from("--model"));
        args.push(OsString::from(&plan.preset.model_id));
    }
    if policy.inject_effort
        && let Some(effort) = &plan.preset.native_effort
    {
        args.push(OsString::from("-c"));
        args.push(quoted_config("model_reasoning_effort", effort));
    }
    if policy.inject_service_tier
        && let Some(service_tier) = &plan.preset.service_tier
    {
        args.push(OsString::from("-c"));
        args.push(quoted_config("service_tier", service_tier));
    }
    args.extend(plan.injected_args.iter().cloned());
    args.extend(plan.inherited_args.iter().cloned());
    if let Some(prompt) = &plan.prompt {
        args.push(prompt.clone());
    }
    args
}

fn preview_arg(argument: &OsStr) -> String {
    let text = argument.to_string_lossy();
    if !text.is_empty()
        && text
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"_+-./:=,@".contains(&byte))
    {
        return text.into_owned();
    }
    format!("'{}'", text.replace('\'', "'\\''"))
}

#[must_use]
pub fn preview(program: &OsStr, args: &[OsString]) -> String {
    std::iter::once(program)
        .chain(args.iter().map(OsString::as_os_str))
        .map(preview_arg)
        .collect::<Vec<_>>()
        .join(" ")
}

pub fn execute(plan: &LaunchPlan, policy: InjectionPolicy) -> Result<ExitCode, AppError> {
    let args = materialize_args(plan, policy);
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    let mut command = Command::new(&plan.codex_binary);
    command
        .args(&args)
        .env("CAUTO_ACTIVE", "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        let source = command.exec();
        Err(AppError::LaunchFailed {
            path: plan.codex_binary.clone(),
            source,
        })
    }
    #[cfg(not(unix))]
    {
        let status = command.status().map_err(|source| AppError::LaunchFailed {
            path: plan.codex_binary.clone(),
            source,
        })?;
        let code = status
            .code()
            .and_then(|value| u8::try_from(value).ok())
            .unwrap_or(1);
        Ok(ExitCode::from(code))
    }
}
