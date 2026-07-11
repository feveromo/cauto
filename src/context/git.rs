use std::ffi::OsStr;
use std::io::Read;
use std::process::{Command, Stdio};

use serde::Serialize;
use wait_timeout::ChildExt;

use crate::config::schema::TimeoutMillis;

#[cfg(unix)]
use std::os::unix::process::CommandExt;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum GitState {
    Clean,
    Dirty,
    Unknown,
    NotRepository,
}

#[derive(Clone, Debug)]
pub struct GitContext {
    pub branch: Option<String>,
    pub state: GitState,
    pub warning: Option<String>,
}

#[cfg(unix)]
fn terminate(child: &mut std::process::Child) {
    use nix::sys::signal::{Signal, killpg};
    use nix::unistd::Pid;

    let _ = killpg(Pid::from_raw(child.id() as i32), Signal::SIGKILL);
    let _ = child.wait();
}

#[cfg(not(unix))]
fn terminate(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

pub fn inspect(root: &std::path::Path, has_git: bool, timeout: TimeoutMillis) -> GitContext {
    inspect_with_program(root, has_git, timeout, OsStr::new("git"))
}

/// Alternate Git executable entrypoint used by deterministic integration tests.
#[doc(hidden)]
pub fn inspect_with_program(
    root: &std::path::Path,
    has_git: bool,
    timeout: TimeoutMillis,
    program: &OsStr,
) -> GitContext {
    if !has_git {
        return GitContext {
            branch: None,
            state: GitState::NotRepository,
            warning: None,
        };
    }
    let mut command = Command::new(program);
    command
        .arg("status")
        .arg("--porcelain=v2")
        .arg("--branch")
        .arg("--untracked-files=normal")
        .current_dir(root)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    #[cfg(unix)]
    command.process_group(0);
    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(error) => {
            return GitContext {
                branch: None,
                state: GitState::Unknown,
                warning: Some(format!("git status unavailable: {error}")),
            };
        }
    };
    let stdout = child.stdout.take().expect("piped git stdout is available");
    let reader = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.take(256 * 1024).read_to_end(&mut bytes);
        bytes
    });
    match child.wait_timeout(timeout.duration()) {
        Ok(Some(status)) if status.success() => {}
        Ok(Some(status)) => {
            let _ = reader.join();
            return GitContext {
                branch: None,
                state: GitState::Unknown,
                warning: Some(format!("git status exited with {status}")),
            };
        }
        Ok(None) => {
            terminate(&mut child);
            let _ = reader.join();
            return GitContext {
                branch: None,
                state: GitState::Unknown,
                warning: Some(format!("git status exceeded {}ms", timeout.get())),
            };
        }
        Err(error) => {
            terminate(&mut child);
            let _ = reader.join();
            return GitContext {
                branch: None,
                state: GitState::Unknown,
                warning: Some(format!("git status wait failed: {error}")),
            };
        }
    }
    let stdout = match reader.join() {
        Ok(stdout) => stdout,
        Err(_) => {
            return GitContext {
                branch: None,
                state: GitState::Unknown,
                warning: Some("git status output reader failed".into()),
            };
        }
    };
    let text = String::from_utf8_lossy(&stdout);
    let mut branch = None;
    let mut dirty = false;
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("# branch.head ") {
            if value != "(detached)" {
                branch = Some(value.to_owned());
            }
        } else if !line.starts_with('#') && !line.trim().is_empty() {
            dirty = true;
        }
    }
    GitContext {
        branch,
        state: if dirty {
            GitState::Dirty
        } else {
            GitState::Clean
        },
        warning: dirty.then(|| {
            "worktree is dirty; cauto will not mutate it and Codex should preserve user changes"
                .into()
        }),
    }
}
