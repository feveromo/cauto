//! Bounded repository, Git, and AGENTS.md context extraction.

pub mod agents;
pub mod git;
pub mod repository;

use std::path::Path;

use crate::config::schema::TimeoutMillis;
use crate::error::AppError;

pub use agents::AgentsContext;
pub use git::{GitContext, GitState};
pub use repository::RepositoryContext;

#[derive(Clone, Debug)]
pub struct ContextSnapshot {
    pub repository: RepositoryContext,
    pub git: GitContext,
    pub agents: AgentsContext,
}

/// Collects bounded metadata without recursively scanning repository contents.
pub fn collect(
    explicit_repo: Option<&Path>,
    working_directory: &Path,
    git_timeout: TimeoutMillis,
) -> Result<ContextSnapshot, AppError> {
    let repository = repository::discover(explicit_repo, working_directory)?;
    let git = git::inspect(&repository.root, repository.has_git, git_timeout);
    let agents = agents::read_applicable(&repository.root, &repository.working_directory);
    Ok(ContextSnapshot {
        repository,
        git,
        agents,
    })
}
