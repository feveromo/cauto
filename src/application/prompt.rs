use std::ffi::{OsStr, OsString};
use std::io::{IsTerminal, Read};
use std::path::PathBuf;

use crate::cli::RouteArgs;
use crate::error::AppError;
use crate::routing::LaunchMode;

#[derive(Clone, Debug)]
pub(super) struct PromptInput {
    pub original: Option<OsString>,
    pub analysis: String,
    pub valid_utf8: bool,
    pub byte_length: usize,
}

#[cfg(unix)]
fn os_bytes(value: &OsStr) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    value.as_bytes()
}

#[cfg(not(unix))]
fn os_bytes(value: &OsStr) -> &[u8] {
    value.to_string_lossy().as_bytes()
}

#[cfg(unix)]
fn os_from_bytes(bytes: Vec<u8>) -> Result<OsString, AppError> {
    use std::os::unix::ffi::OsStringExt;
    Ok(OsString::from_vec(bytes))
}

#[cfg(not(unix))]
fn os_from_bytes(bytes: Vec<u8>) -> Result<OsString, AppError> {
    String::from_utf8(bytes)
        .map(OsString::from)
        .map_err(|error| AppError::InvalidArguments(format!("prompt was not UTF-8: {error}")))
}

pub(super) fn acquire(args: &RouteArgs, mode: LaunchMode) -> Result<PromptInput, AppError> {
    let source_count = usize::from(args.task.is_some())
        + usize::from(args.prompt.is_some())
        + usize::from(args.prompt_file.is_some())
        + usize::from(args.stdin);
    if source_count > 1 {
        return Err(AppError::InvalidArguments(
            "choose exactly one prompt source: positional, --prompt, --prompt-file, or --stdin"
                .into(),
        ));
    }
    let original = if let Some(task) = &args.task {
        Some(task.clone())
    } else if let Some(prompt) = &args.prompt {
        Some(OsString::from(prompt))
    } else if let Some(path) = &args.prompt_file {
        Some(os_from_bytes(std::fs::read(path).map_err(|source| {
            AppError::Io {
                path: path.clone(),
                source,
            }
        })?)?)
    } else if args.stdin {
        let mut bytes = Vec::new();
        std::io::stdin()
            .read_to_end(&mut bytes)
            .map_err(|source| AppError::Io {
                path: PathBuf::from("<stdin>"),
                source,
            })?;
        Some(os_from_bytes(bytes)?)
    } else {
        None
    };
    if original.is_none() && (mode == LaunchMode::Exec || !std::io::stdin().is_terminal()) {
        return Err(AppError::PromptMissing);
    }
    let (analysis, valid_utf8, byte_length) = match &original {
        Some(prompt) => (
            prompt.to_string_lossy().into_owned(),
            prompt.to_str().is_some(),
            os_bytes(prompt).len(),
        ),
        None => (String::new(), true, 0),
    };
    Ok(PromptInput {
        original,
        analysis,
        valid_utf8,
        byte_length,
    })
}
