# cauto

`cauto` is a fast, repository-aware session-start router for the native OpenAI
Codex CLI. It scores a task, resolves the lowest capable installed model and
reasoning effort, explains the choice, records a redacted decision, and then
replaces itself with the real `codex` process on Unix.

It is a transparent launcher. It reuses the native CLI's existing ChatGPT
authentication, subscription allowance, config, profiles, MCP servers, rules,
skills, permissions, sandbox, and terminal behavior. It is not an API proxy,
alternate TUI, provider bridge, or billing layer, and it never configures an API
key.

## Install

Rust 1.89 or newer is required. This host currently builds with stable Rust.

```bash
scripts/install.sh
```

The installer runs formatting, Clippy, and tests before:

```bash
cargo install --path . --locked --force
```

Use `scripts/install.sh --dry-run` to preview or `--skip-tests` when the gates
already ran. For development, use `cargo run -- --dry-run "task"`. Update by
pulling the repository and rerunning the installer. Uninstall with
`cargo uninstall cauto`.

## Daily Use

```bash
cauto "diagnose the intermittent lifecycle bug and live-validate the fix"
cauto exec "run a bounded non-interactive implementation task"
cauto explain "reverse engineer this packet handler"
cauto --dry-run "fix a typo in README.md"
```

An interactive invocation without a prompt opens native Codex using the
configured default route. A non-interactive invocation without an explicit
prompt source fails instead of classifying an empty string as cheap work.

Prompt sources are positional text, `--prompt`, `--prompt-file`, or `--stdin`.
Use exactly one. Native arguments require `--`, so boundaries are unambiguous:

```bash
cauto --prompt "research upstream behavior" -- --search
cauto --prompt-file task.txt -- --image "screen one.png" --image screen-two.png
```

Everything after `--` is forwarded as the original `OsString` sequence.

## Overrides

Automatic choices can be constrained with:

```bash
cauto --family terra --effort medium "implement the known contract"
cauto --model gpt-5.6-sol --effort high "investigate this bug"
cauto --fast "task"
cauto --no-fast "task"
cauto --inherit-fast "task"
```

Forwarded native `--model`, `-m`, profiles, `-c model=...`,
`model_reasoning_effort=...`, and `service_tier=...` are detected without
rewriting or reordering them. Explicit choices win. Conflicting cauto and native
forms are rejected instead of injecting duplicates. Fast defaults to inherit
and is never enabled merely because a task is simple.

Automatic Ultra requires proven installed support, high complexity,
parallelizability of at least three, meaningful independent tracks, and
explicit delegation authorization through `--allow-ultra`, the prompt, or
applicable instructions. An exact user `--effort ultra` override is itself an
explicit request but still requires proven catalog and launch-path support. Max
and Ultra are never aliases for `xhigh`.

## Configuration

Configuration is loaded from:

1. `~/.config/cauto/config.toml`
2. `<repo-root>/.cauto.toml`

CLI and explicit native overrides are applied above those typed layers. Project
policy cannot force Fast. Example user configuration:

```toml
version = 1
classifier = "auto"
classifier_confidence_threshold = 0.72
default_model = "gpt-5.6-sol"
default_effort = "medium"
fast_mode = "inherit"
ultra_requires_opt_in = true
allow_automatic_downgrade = true
log_raw_prompts = false
catalog_cache_hours = 12
git_timeout_ms = 250
catalog_timeout_ms = 2500
classifier_timeout_seconds = 45

[weights]
scope = 20
ambiguity = 20
cost_of_being_wrong = 20
runtime_dependence = 15
architectural_depth = 15
verification_burden = 10
```

Project rules contain phrases, repository-relative globs, bounded dimension
deltas, optional family/effort floors or ceilings, a confidence delta, and a
human reason. They cannot execute commands, read secrets, or inject Codex argv.
Phrases compile into one case-insensitive Aho-Corasick automaton and paths into
one GlobSet per invocation.

## Routing

The pure core scores scope, ambiguity, cost of being wrong, runtime dependence,
architectural depth, verification burden, and parallelizability from 0 through
4. The first six dimensions are normalized with integer weights to 0 through
100. Parallelizability only affects Ultra candidacy.

Base effort thresholds are Low 0-20, Medium 21-45, High 46-68, Extra High
69-84, and Max candidate 85-100. Luna is reserved for clear, mechanical,
low-risk work; Terra handles bounded everyday coding; Sol handles ambiguity,
runtime risk, architecture, protocols, pathing, rendering, concurrency, and
high verification burden. Policy floors, ceilings, conflicts, explicit
overrides, catalog support, and downgrade rules are then applied.

Threshold hysteresis reads at most the last 256 KiB of redacted decision
history and retains the previous repository effort only inside the configured
boundary margin. It never reads a prior raw prompt because none is stored.

`cauto models` shows the installed catalog. Add `--refresh`, `--bundled`,
`--include-hidden`, or `--json`. `cauto doctor` reports the resolved binary,
Codex version, paths, cache age/source, aliases, classifier usability, and
actual Max/Ultra support without printing secrets.

## Classifier

The deterministic route runs first. Luna classification is considered only for
low-confidence, conflicting, or unmatched tasks, or with
`--classifier always`. Disable it with `--no-classifier`, `--classifier never`,
or `--offline`.

The classifier uses native `codex exec` and saved authentication in a private
temporary directory, read-only sandbox, low effort, strict JSON schema, and a
separate timed process group. It receives bounded metadata, not file contents
or environment data. Failures fall back to deterministic routing. It can never
authorize Ultra or weaken explicit project safety floors.

## Privacy And History

Raw prompts are never stored. Decision records under
`~/.local/state/cauto/decisions.jsonl` contain a SHA-256 prompt digest, byte
length, bounded scores, rule IDs, selected capability, outcome category, and
sanitized argv with the prompt removed. Unknown `-c` values are redacted.
Cache/state directories and files use user-only permissions where supported.

```bash
cauto feedback right
cauto feedback overkill
cauto feedback underpowered
cauto feedback failed-for-other-reason
cauto report
```

Feedback is reported but does not automatically retune version 1.

## Troubleshooting

- Run `cauto doctor` when Codex, profile, or catalog discovery looks wrong.
- Run `cauto models --refresh` after a native Codex update.
- Use `--codex-bin PATH` or `CODEX_BIN` if PATH resolves the wrong entrypoint.
- Use `--allow-downgrade` only when an explicit unsupported preset may fall
  back.
- Use `--no-project-policy` to diagnose a repository policy independently.
- A dirty worktree produces a preservation warning but never mutates files or
  forces Max/Ultra.

Benchmark a release build with `scripts/bench.sh`. It measures help, explain,
deterministic dry-run, a 20 KB prompt, cached catalog loading, route-to-command
planning, and pure scoring while excluding the classifier and final Codex
runtime.
