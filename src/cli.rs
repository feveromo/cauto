//! Command-line grammar only; application behavior lives in `application`.

use std::ffi::OsString;
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand, ValueEnum};
use clap_complete::Shell;

#[derive(Debug, Parser)]
#[command(
    name = "cauto",
    version,
    about = "Repository-aware automatic model and reasoning router for native Codex",
    long_about = None
)]
pub struct Cli {
    #[command(flatten)]
    pub global: GlobalArgs,
    #[command(subcommand)]
    pub command: Option<Commands>,
    #[command(flatten)]
    pub route: RouteArgs,
}

#[derive(Clone, Debug, Args)]
pub struct GlobalArgs {
    #[arg(long, global = true, value_name = "PATH")]
    pub repo: Option<PathBuf>,
    #[arg(long, global = true, value_name = "PATH")]
    pub codex_bin: Option<PathBuf>,
    #[arg(long, global = true, value_name = "NAME")]
    pub profile: Option<String>,
    /// Emit machine-readable output for non-TUI commands.
    #[arg(long, global = true)]
    pub json: bool,
    #[arg(long, global = true)]
    pub verbose: bool,
    #[arg(long, global = true)]
    pub quiet: bool,
    #[arg(long, global = true)]
    pub no_color: bool,
}

#[derive(Clone, Debug, Default, Args)]
pub struct RouteArgs {
    #[arg(value_name = "PROMPT")]
    pub task: Option<OsString>,
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
    #[arg(long, value_parser = ["sol", "terra", "luna"])]
    pub family: Option<String>,
    #[arg(
        long,
        value_parser = ["minimal", "low", "medium", "high", "xhigh", "max", "ultra"]
    )]
    pub effort: Option<String>,
    #[arg(long, conflicts_with_all = ["no_fast", "inherit_fast"])]
    pub fast: bool,
    #[arg(long, conflicts_with_all = ["fast", "inherit_fast"])]
    pub no_fast: bool,
    #[arg(long, conflicts_with_all = ["fast", "no_fast"])]
    pub inherit_fast: bool,
    #[arg(long)]
    pub allow_ultra: bool,
    #[arg(long)]
    pub allow_downgrade: bool,
    #[arg(long, value_parser = ["auto", "always", "never"])]
    pub classifier: Option<String>,
    #[arg(long, conflicts_with = "classifier")]
    pub no_classifier: bool,
    /// Allow a classifier task to run while previewing with --dry-run or explain.
    #[arg(long, conflicts_with = "no_classifier")]
    pub run_classifier: bool,
    #[arg(long)]
    pub offline: bool,
    #[arg(long)]
    pub dry_run: bool,
    #[arg(long)]
    pub print_command: bool,
    #[arg(long, value_name = "TEXT")]
    pub prompt: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub prompt_file: Option<PathBuf>,
    #[arg(long)]
    pub stdin: bool,
    #[arg(long)]
    pub no_project_policy: bool,
    #[arg(last = true, value_name = "CODEX_ARGS", allow_hyphen_values = true)]
    pub forwarded: Vec<OsString>,
}

#[derive(Clone, Debug, Default, Args)]
pub struct AgentRouteArgs {
    #[arg(value_name = "PROMPT")]
    pub task: Option<OsString>,
    #[arg(long, value_name = "MODEL")]
    pub model: Option<String>,
    #[arg(long, value_parser = ["sol", "terra", "luna"])]
    pub family: Option<String>,
    #[arg(
        long,
        value_parser = ["minimal", "low", "medium", "high", "xhigh", "max", "ultra"]
    )]
    pub effort: Option<String>,
    #[arg(long, conflicts_with_all = ["no_fast", "inherit_fast"])]
    pub fast: bool,
    #[arg(long, conflicts_with_all = ["fast", "inherit_fast"])]
    pub no_fast: bool,
    #[arg(long, conflicts_with_all = ["fast", "no_fast"])]
    pub inherit_fast: bool,
    #[arg(long)]
    pub allow_ultra: bool,
    #[arg(long)]
    pub allow_downgrade: bool,
    #[arg(long, value_parser = ["auto", "always", "never"])]
    pub classifier: Option<String>,
    #[arg(long, conflicts_with = "classifier")]
    pub no_classifier: bool,
    #[arg(long)]
    pub offline: bool,
    #[arg(long, value_name = "TEXT")]
    pub prompt: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub prompt_file: Option<PathBuf>,
    #[arg(long)]
    pub stdin: bool,
    #[arg(long)]
    pub no_project_policy: bool,
    #[arg(last = true, value_name = "CODEX_ARGS", allow_hyphen_values = true)]
    pub forwarded: Vec<OsString>,
}

impl From<AgentRouteArgs> for RouteArgs {
    fn from(args: AgentRouteArgs) -> Self {
        Self {
            task: args.task,
            model: args.model,
            family: args.family,
            effort: args.effort,
            fast: args.fast,
            no_fast: args.no_fast,
            inherit_fast: args.inherit_fast,
            allow_ultra: args.allow_ultra,
            allow_downgrade: args.allow_downgrade,
            classifier: args.classifier,
            no_classifier: args.no_classifier,
            run_classifier: false,
            offline: args.offline,
            dry_run: false,
            print_command: false,
            prompt: args.prompt,
            prompt_file: args.prompt_file,
            stdin: args.stdin,
            no_project_policy: args.no_project_policy,
            forwarded: args.forwarded,
        }
    }
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Run the native Codex TUI through the adaptive App Server router.
    Agent(AgentArgs),
    /// Run native Codex in non-interactive exec mode.
    Exec(RouteArgs),
    /// Explain a route without launching Codex.
    Explain(RouteArgs),
    /// List installed model capabilities.
    Models(ModelsArgs),
    /// Diagnose cauto, Codex, and catalog state.
    Doctor,
    /// Attach feedback to the latest decision for this repository.
    Feedback {
        #[arg(value_enum)]
        kind: FeedbackArg,
    },
    /// Summarize redacted decision history.
    Report,
    /// Analyze feedback and manage repository-local routing calibration.
    Tune(TuneArgs),
    /// Generate shell completions.
    Completions {
        #[arg(value_enum)]
        shell: Shell,
    },
    #[command(hide = true)]
    BenchScore {
        #[arg(long, default_value_t = 1_000_000)]
        iterations: u64,
    },
    #[command(hide = true)]
    BenchProcess {
        #[arg(long, default_value_t = 100)]
        iterations: u64,
        #[arg(value_name = "PROGRAM")]
        program: PathBuf,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<OsString>,
    },
    #[command(hide = true)]
    BenchCore {
        #[arg(long)]
        policy: PathBuf,
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long, default_value_t = 1_000)]
        iterations: u64,
    },
}

#[derive(Clone, Debug, Default, Args)]
pub struct AgentArgs {
    /// Resume a persisted Codex thread by id.
    #[arg(long, value_name = "THREAD_ID")]
    pub resume: Option<String>,
    #[command(flatten)]
    pub route: AgentRouteArgs,
}

#[derive(Clone, Copy, Debug, Default, Args)]
pub struct TuneArgs {
    /// Apply the eligible recommendation for the selected repository.
    #[arg(long, conflicts_with = "reset")]
    pub apply: bool,
    /// Remove only the selected repository's applied calibration.
    #[arg(long, conflicts_with = "apply")]
    pub reset: bool,
}

#[derive(Clone, Debug, Default, Args)]
pub struct ModelsArgs {
    #[arg(long)]
    pub refresh: bool,
    #[arg(long)]
    pub bundled: bool,
    #[arg(long)]
    pub include_hidden: bool,
}

#[derive(Clone, Copy, Debug, ValueEnum)]
pub enum FeedbackArg {
    Right,
    Overkill,
    Underpowered,
    FailedForOtherReason,
}
