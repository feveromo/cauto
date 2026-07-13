//! Application orchestration split by command responsibility.

mod agent;
mod commands;
mod decision;
mod prompt;
mod route;

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use clap::CommandFactory;

use crate::cli::{Cli, Commands, GlobalArgs, RouteArgs};
use crate::codex::args::ExplicitNativeOverrides;
use crate::codex::binary::{NativeProcessRunner, resolve as resolve_codex};
use crate::codex::catalog::{CatalogManager, CatalogRequest, ModelCatalog};
use crate::config::{LoadedConfig, load as load_config};
use crate::context::{ContextSnapshot, git, repository};
use crate::error::AppError;
use crate::paths::CautoPaths;
use crate::routing::LaunchMode;

pub fn run(cli: Cli) -> Result<ExitCode, AppError> {
    match cli.command {
        None => route::run_route(&cli.global, cli.route, LaunchMode::Interactive, false),
        Some(Commands::Agent(args)) => agent::run(&cli.global, args),
        Some(Commands::Exec(args)) => route::run_route(
            &cli.global,
            merge_route_args(&cli.route, args)?,
            LaunchMode::Exec,
            false,
        ),
        Some(Commands::Explain(mut args)) => {
            args.dry_run = true;
            route::run_route(
                &cli.global,
                merge_route_args(&cli.route, args)?,
                LaunchMode::Interactive,
                true,
            )
        }
        Some(Commands::Models(args)) => commands::run_models(&cli.global, args),
        Some(Commands::Doctor) => commands::run_doctor(&cli.global),
        Some(Commands::Feedback { kind }) => commands::run_feedback(&cli.global, kind),
        Some(Commands::Report) => commands::run_report(&cli.global),
        Some(Commands::Tune(args)) => commands::run_tune(&cli.global, args),
        Some(Commands::Completions { shell }) => {
            let mut command = Cli::command();
            clap_complete::generate(shell, &mut command, "cauto", &mut std::io::stdout());
            Ok(ExitCode::SUCCESS)
        }
        Some(Commands::BenchScore { iterations }) => commands::run_score_benchmark(iterations),
        Some(Commands::BenchProcess {
            iterations,
            program,
            args,
        }) => commands::run_process_benchmark(&program, &args, iterations),
        Some(Commands::BenchCore {
            policy,
            catalog,
            iterations,
        }) => commands::run_core_benchmark(&policy, &catalog, iterations),
    }
}

fn merge_route_args(root: &RouteArgs, subcommand: RouteArgs) -> Result<RouteArgs, AppError> {
    if root.task.is_some()
        || root.model.is_some()
        || root.family.is_some()
        || root.effort.is_some()
        || root.fast
        || root.no_fast
        || root.inherit_fast
        || root.allow_ultra
        || root.allow_downgrade
        || root.classifier.is_some()
        || root.no_classifier
        || root.run_classifier
        || root.offline
        || root.dry_run
        || root.print_command
        || root.prompt.is_some()
        || root.prompt_file.is_some()
        || root.stdin
        || root.no_project_policy
        || !root.forwarded.is_empty()
    {
        return Err(AppError::InvalidArguments(
            "route options must be placed after the exec or explain subcommand".into(),
        ));
    }
    Ok(subcommand)
}

fn load_context_and_config(
    global: &GlobalArgs,
    route_args: Option<&RouteArgs>,
) -> Result<(ContextSnapshot, LoadedConfig, CautoPaths), AppError> {
    let current = std::env::current_dir().map_err(|source| AppError::Io {
        path: PathBuf::from("."),
        source,
    })?;
    let repository = repository::discover(global.repo.as_deref(), &current)?;
    let paths = CautoPaths::discover()?;
    let project_path = route_args
        .is_none_or(|args| !args.no_project_policy)
        .then(|| repository.root.join(".cauto.toml"));
    let loaded = load_config(&paths.user_config(), project_path.as_deref())?;
    let git = git::inspect(
        &repository.root,
        repository.has_git,
        loaded.config.git_timeout,
    );
    let agents =
        crate::context::agents::read_applicable(&repository.root, &repository.working_directory);
    Ok((
        ContextSnapshot {
            repository,
            git,
            agents,
        },
        loaded,
        paths,
    ))
}

fn resolve_installation(
    global: &GlobalArgs,
    native: &ExplicitNativeOverrides,
) -> Result<crate::codex::CodexInstallation, AppError> {
    if let (Some(cauto_profile), Some(native_profile)) = (&global.profile, &native.profile)
        && cauto_profile != native_profile
    {
        return Err(AppError::InvalidArguments(format!(
            "--profile {cauto_profile:?} conflicts with forwarded profile {native_profile:?}"
        )));
    }
    resolve_codex(
        global.codex_bin.as_deref(),
        global.profile.as_deref().or(native.profile.as_deref()),
    )
}

fn catalog_for(
    paths: &CautoPaths,
    loaded: &LoadedConfig,
    installation: &crate::codex::CodexInstallation,
    refresh: bool,
    bundled: bool,
) -> Result<ModelCatalog, AppError> {
    let runner = NativeProcessRunner;
    let manager = CatalogManager {
        paths,
        runner: &runner,
    };
    manager.load(
        &CatalogRequest {
            installation: installation.clone(),
            timeout: loaded.config.catalog_timeout.duration(),
            max_age: Duration::from_secs(loaded.config.catalog_cache_hours.get() * 3_600),
            include_hidden: true,
        },
        refresh,
        bundled,
    )
}
