use std::path::{Path, PathBuf};

use crate::error::AppError;

const MAX_TOP_LEVEL_ENTRIES: usize = 64;
const MAX_NAME_BYTES: usize = 256;

#[derive(Clone, Debug)]
pub struct RepositoryContext {
    pub root: PathBuf,
    pub name: String,
    pub working_directory: PathBuf,
    pub relative_working_directory: PathBuf,
    pub top_level_names: Vec<String>,
    pub has_git: bool,
}

pub fn discover(
    explicit_repo: Option<&Path>,
    working_directory: &Path,
) -> Result<RepositoryContext, AppError> {
    let working_directory = working_directory
        .canonicalize()
        .unwrap_or_else(|_| working_directory.to_path_buf());
    let root = if let Some(explicit) = explicit_repo {
        if !explicit.is_dir() {
            return Err(AppError::RepositoryDiscovery {
                path: explicit.to_path_buf(),
                message: "the explicit repository path is not a directory".into(),
            });
        }
        explicit
            .canonicalize()
            .unwrap_or_else(|_| explicit.to_path_buf())
    } else {
        working_directory
            .ancestors()
            .find(|candidate| candidate.join(".git").exists())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| working_directory.clone())
    };
    let relative_working_directory = working_directory
        .strip_prefix(&root)
        .map(Path::to_path_buf)
        .unwrap_or_default();
    let name = root
        .file_name()
        .map(|value| value.to_string_lossy().into_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| root.to_string_lossy().into_owned());
    let mut top_level_names = Vec::with_capacity(32);
    if let Ok(entries) = std::fs::read_dir(&root) {
        for entry in entries.flatten().take(MAX_TOP_LEVEL_ENTRIES) {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.len() <= MAX_NAME_BYTES && name != ".git" && name != ".codegraph" {
                top_level_names.push(name);
            }
        }
        top_level_names.sort_unstable();
    }
    Ok(RepositoryContext {
        has_git: root.join(".git").exists(),
        root,
        name,
        working_directory,
        relative_working_directory,
        top_level_names,
    })
}
