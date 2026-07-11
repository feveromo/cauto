use std::ffi::OsString;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use thiserror::Error;
use wait_timeout::ChildExt;

use crate::codex::args::quoted_config;
use crate::codex::binary::CodexInstallation;
use crate::routing::{ClassifierMode, Confidence};

use super::schema::{ClassifierAssessment, output_schema};

#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt};
#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[derive(Debug, Error)]
pub enum ClassifierError {
    #[error("nested classifier invocation was refused")]
    Nested,
    #[error("temporary workspace failed: {0}")]
    Temporary(String),
    #[error("classifier launch failed: {0}")]
    Launch(String),
    #[error("classifier timed out")]
    Timeout,
    #[error("classifier exited unsuccessfully")]
    Exit,
    #[error("classifier output is invalid: {0}")]
    InvalidOutput(String),
}

#[derive(Clone, Debug)]
pub struct ClassifierRun {
    pub assessment: ClassifierAssessment,
    pub category: String,
}

#[allow(clippy::too_many_arguments)]
#[must_use]
pub fn should_run(
    mode: ClassifierMode,
    confidence: Confidence,
    threshold_basis_points: u16,
    has_conflicts: bool,
    matched_rules: usize,
    prompt_is_utf8: bool,
    explicit_route_complete: bool,
    offline: bool,
    luna_available: bool,
) -> bool {
    if mode == ClassifierMode::Never
        || offline
        || !luna_available
        || !prompt_is_utf8
        || explicit_route_complete
        || std::env::var_os("CAUTO_CLASSIFIER").is_some()
    {
        return false;
    }
    mode == ClassifierMode::Always
        || has_conflicts
        || matched_rules == 0 && confidence.basis_points() < threshold_basis_points
}

struct TemporaryWorkspace {
    path: PathBuf,
}

impl Drop for TemporaryWorkspace {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn temporary_workspace() -> Result<TemporaryWorkspace, ClassifierError> {
    let root = std::env::temp_dir();
    for attempt in 0..16_u8 {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let path = root.join(format!(
            "cauto-classifier-{}-{nonce}-{attempt}",
            std::process::id()
        ));
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        builder.mode(0o700);
        match builder.create(&path) {
            Ok(()) => return Ok(TemporaryWorkspace { path }),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(ClassifierError::Temporary(error.to_string())),
        }
    }
    Err(ClassifierError::Temporary(
        "could not allocate a unique directory".into(),
    ))
}

fn write_private(path: &Path, bytes: &[u8]) -> Result<(), ClassifierError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options
        .open(path)
        .map_err(|error| ClassifierError::Temporary(error.to_string()))?;
    file.write_all(bytes)
        .and_then(|()| file.flush())
        .map_err(|error| ClassifierError::Temporary(error.to_string()))
}

#[cfg(unix)]
fn terminate_group(child: &mut std::process::Child) {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    let group = Pid::from_raw(child.id() as i32);
    let _ = killpg(group, Signal::SIGTERM);
    if child
        .wait_timeout(Duration::from_millis(250))
        .ok()
        .flatten()
        .is_none()
    {
        let _ = killpg(group, Signal::SIGKILL);
    }
    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate_group(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub fn run(
    installation: &CodexInstallation,
    luna_model: &str,
    classifier_prompt: &str,
    timeout: Duration,
) -> Result<ClassifierRun, ClassifierError> {
    if std::env::var_os("CAUTO_CLASSIFIER").is_some() {
        return Err(ClassifierError::Nested);
    }
    let workspace = temporary_workspace()?;
    let schema_path = workspace.path.join("schema.json");
    let result_path = workspace.path.join("result.json");
    let schema = serde_json::to_vec(&output_schema())
        .map_err(|error| ClassifierError::Temporary(error.to_string()))?;
    write_private(&schema_path, &schema)?;
    let mut args = Vec::with_capacity(20);
    args.push(OsString::from("exec"));
    args.push(OsString::from("--model"));
    args.push(OsString::from(luna_model));
    args.push(OsString::from("--ephemeral"));
    args.push(OsString::from("--sandbox"));
    args.push(OsString::from("read-only"));
    args.push(OsString::from("--skip-git-repo-check"));
    args.push(OsString::from("--cd"));
    args.push(workspace.path.as_os_str().to_owned());
    args.push(OsString::from("--output-schema"));
    args.push(schema_path.as_os_str().to_owned());
    args.push(OsString::from("--output-last-message"));
    args.push(result_path.as_os_str().to_owned());
    args.push(OsString::from("-c"));
    args.push(quoted_config("model_reasoning_effort", "low"));
    args.push(OsString::from(classifier_prompt));
    let mut command = Command::new(&installation.binary);
    command
        .args(args)
        .current_dir(&workspace.path)
        .env("CAUTO_CLASSIFIER", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = command
        .spawn()
        .map_err(|error| ClassifierError::Launch(error.to_string()))?;
    let status = match child
        .wait_timeout(timeout)
        .map_err(|error| ClassifierError::Launch(error.to_string()))?
    {
        Some(status) => status,
        None => {
            terminate_group(&mut child);
            return Err(ClassifierError::Timeout);
        }
    };
    if !status.success() {
        return Err(ClassifierError::Exit);
    }
    let bytes = std::fs::read(&result_path)
        .map_err(|error| ClassifierError::InvalidOutput(error.to_string()))?;
    if bytes.len() > 64 * 1024 {
        return Err(ClassifierError::InvalidOutput(
            "output exceeded 64 KiB".into(),
        ));
    }
    let assessment = ClassifierAssessment::parse(&bytes).map_err(ClassifierError::InvalidOutput)?;
    Ok(ClassifierRun {
        assessment,
        category: "success".into(),
    })
}
