use std::ffi::{OsStr, OsString};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, UNIX_EPOCH};

use directories::BaseDirs;
use sha2::{Digest, Sha256};
use thiserror::Error;
use wait_timeout::ChildExt;

use crate::error::AppError;

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

const MAX_PROCESS_OUTPUT: u64 = 16 * 1024 * 1024;

#[derive(Clone, Debug)]
pub struct CodexInstallation {
    pub binary: PathBuf,
    pub canonical_binary: PathBuf,
    pub fingerprint: String,
    pub codex_home: PathBuf,
    pub codex_home_hash: String,
    pub profile: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ProcessRequest {
    pub program: PathBuf,
    pub args: Vec<OsString>,
    pub current_dir: Option<PathBuf>,
    pub environment: Vec<(OsString, OsString)>,
    pub timeout: Duration,
}

#[derive(Clone, Debug)]
pub struct ProcessOutput {
    pub status_code: Option<i32>,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum ProcessError {
    #[error("process timed out after {0}ms")]
    Timeout(u128),
    #[error("process I/O failed: {0}")]
    Io(#[from] std::io::Error),
}

pub trait ProcessRunner {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessError>;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct NativeProcessRunner;

fn read_bounded<R: Read>(reader: R) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    reader.take(MAX_PROCESS_OUTPUT).read_to_end(&mut bytes)?;
    Ok(bytes)
}

impl ProcessRunner for NativeProcessRunner {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        let mut command = Command::new(&request.program);
        command
            .args(&request.args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(current_dir) = &request.current_dir {
            command.current_dir(current_dir);
        }
        for (key, value) in &request.environment {
            command.env(key, value);
        }
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("piped stdout is available");
        let stderr = child.stderr.take().expect("piped stderr is available");
        let stdout_reader = std::thread::spawn(move || read_bounded(stdout));
        let stderr_reader = std::thread::spawn(move || read_bounded(stderr));
        let status = match child.wait_timeout(request.timeout)? {
            Some(status) => status,
            None => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_reader.join();
                let _ = stderr_reader.join();
                return Err(ProcessError::Timeout(request.timeout.as_millis()));
            }
        };
        let stdout = stdout_reader
            .join()
            .map_err(|_| std::io::Error::other("stdout reader panicked"))??;
        let stderr = stderr_reader
            .join()
            .map_err(|_| std::io::Error::other("stderr reader panicked"))??;
        Ok(ProcessOutput {
            status_code: status.code(),
            stdout,
            stderr,
        })
    }
}

fn executable(path: &Path) -> bool {
    let Ok(metadata) = path.metadata() else {
        return false;
    };
    if !metadata.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        metadata.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn path_lookup(name: &OsStr) -> Option<PathBuf> {
    if Path::new(name).components().count() > 1 {
        let path = PathBuf::from(name);
        return executable(&path).then_some(path);
    }
    let search = std::env::var_os("PATH")?;
    for directory in std::env::split_paths(&search) {
        let candidate = directory.join(name);
        if executable(&candidate) {
            return Some(candidate);
        }
        #[cfg(windows)]
        {
            let candidate = directory.join(format!("{}.exe", name.to_string_lossy()));
            if executable(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(unix)]
fn update_os_hash(hasher: &mut Sha256, value: &OsStr) {
    hasher.update(value.as_bytes());
}

#[cfg(not(unix))]
fn update_os_hash(hasher: &mut Sha256, value: &OsStr) {
    hasher.update(value.to_string_lossy().as_bytes());
}

fn hash_path(path: &Path) -> String {
    let mut hasher = Sha256::new();
    update_os_hash(&mut hasher, path.as_os_str());
    format!("{:x}", hasher.finalize())
}

fn update_file_identity(hasher: &mut Sha256, path: &Path, metadata: &std::fs::Metadata) {
    hasher.update([0xff]);
    update_os_hash(hasher, path.as_os_str());
    hasher.update(metadata.len().to_le_bytes());
    if let Ok(modified) = metadata.modified()
        && let Ok(duration) = modified.duration_since(UNIX_EPOCH)
    {
        hasher.update(duration.as_secs().to_le_bytes());
        hasher.update(duration.subsec_nanos().to_le_bytes());
    }
    #[cfg(unix)]
    {
        hasher.update(metadata.dev().to_le_bytes());
        hasher.update(metadata.ino().to_le_bytes());
    }
}

fn related_codex_entrypoints() -> Vec<PathBuf> {
    let Some(search) = std::env::var_os("PATH") else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for directory in std::env::split_paths(&search) {
        for name in ["codex", "codex-openai"] {
            let candidate = directory.join(name);
            if executable(&candidate) {
                paths.push(candidate.canonicalize().unwrap_or(candidate));
            }
        }
    }
    paths.sort_unstable();
    paths.dedup();
    paths
}

/// Resolves the native Codex entrypoint and computes a stable installation fingerprint.
pub fn resolve(
    explicit: Option<&Path>,
    profile: Option<&str>,
) -> Result<CodexInstallation, AppError> {
    let selected = if let Some(path) = explicit {
        path_lookup(path.as_os_str())
    } else if let Some(path) = std::env::var_os("CODEX_BIN") {
        path_lookup(&path)
    } else {
        path_lookup(OsStr::new("codex"))
    }
    .ok_or(AppError::CodexNotFound)?;
    let absolute = if selected.is_absolute() {
        selected
    } else {
        std::env::current_dir().unwrap_or_default().join(selected)
    };
    let canonical_binary = absolute.canonicalize().unwrap_or_else(|_| absolute.clone());
    if let Ok(current) = std::env::current_exe() {
        let current = current.canonicalize().unwrap_or(current);
        if current == canonical_binary {
            return Err(AppError::CodexRecursion(canonical_binary));
        }
    }
    let metadata = canonical_binary.metadata().map_err(|source| AppError::Io {
        path: canonical_binary.clone(),
        source,
    })?;
    let codex_home = if let Some(value) = std::env::var_os("CODEX_HOME") {
        PathBuf::from(value)
    } else {
        BaseDirs::new()
            .map(|base| base.home_dir().join(".codex"))
            .ok_or_else(|| {
                AppError::InvalidArguments(
                    "could not resolve CODEX_HOME or a home directory".into(),
                )
            })?
    };
    let codex_home = codex_home.canonicalize().unwrap_or(codex_home);
    let mut hasher = Sha256::new();
    update_file_identity(&mut hasher, &canonical_binary, &metadata);
    for related in related_codex_entrypoints() {
        if related != canonical_binary
            && let Ok(metadata) = related.metadata()
        {
            update_file_identity(&mut hasher, &related, &metadata);
        }
    }
    update_os_hash(&mut hasher, codex_home.as_os_str());
    let fingerprint = format!("{:x}", hasher.finalize());
    Ok(CodexInstallation {
        binary: absolute,
        canonical_binary,
        fingerprint,
        codex_home_hash: hash_path(&codex_home),
        codex_home,
        profile: profile.map(str::to_owned),
    })
}
