# cauto

> Start Codex at the right capability level—automatically, transparently, and
> without changing how you use Codex.

[![Rust 1.89+](https://img.shields.io/badge/Rust-1.89%2B-dea584?logo=rust&logoColor=white)](https://www.rust-lang.org/)
[![CI](https://github.com/feveromo/cauto/actions/workflows/ci.yml/badge.svg)](https://github.com/feveromo/cauto/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-4c1?logo=opensourceinitiative&logoColor=white)](LICENSE)
[![Native Codex launcher](https://img.shields.io/badge/launches-native%20Codex-101828?logo=openai&logoColor=white)](https://github.com/openai/codex)

`cauto` is a fast, repository-aware router for the native OpenAI Codex CLI. It
scores a task, selects the lowest capable installed model and reasoning effort,
explains the choice, and records a redacted decision. Its adaptive agent mode
does that once for the opening text turn of a native Codex thread, pins the
route for the rest of the session, and learns from later corrections without
requiring manual feedback or tuning commands. Routing is entirely local Rust:
submitting a prompt never starts a hidden model call.

```text
first turn ──> local evidence? ──> cauto route or native route ──> pinned thread
```

It reuses the native CLI's existing ChatGPT authentication, subscription
allowance, config, profiles, MCP servers, rules, skills, permissions, sandbox,
and terminal behavior. `cauto agent` is a loopback-only transparent App Server
transport in front of the native TUI, not an alternate TUI, provider bridge,
billing layer, or OpenAI API client. It never configures an API key.

## Why cauto?

- **Spend capability deliberately.** Straightforward work stays light; risky,
  ambiguous, or architectural work gets the headroom it needs.
- **Keep Codex native.** Your existing authentication, profiles, rules, MCP
  servers, skills, permissions, and terminal flow remain the source of truth.
- **Adapt without a feedback chore.** Clear corrections and explicit route
  changes become bounded signals automatically; weak proxies such as prompt
  length, tool count, or session duration do not.
- **Keep the first turn fast.** Repository state, policy, and the live model
  catalog are prepared before input; pressing Enter performs only in-memory
  feature extraction, rule matching, and selection.
- **See and control every decision.** The initial session route and intentional
  overrides remain visible, preview tools still work, and history contains
  redacted records—never raw prompts.

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
cauto agent
cauto agent "diagnose the intermittent lifecycle bug and live-validate the fix"
cauto agent --resume THREAD_ID
cauto exec "run a bounded non-interactive implementation task"
cauto explain "reverse engineer this packet handler"
cauto --dry-run "fix a typo in README.md"
```

`cauto agent` is the recommended interactive path. It starts the real Codex TUI
through a local transparent transport, routes the first text turn once, pins
that model and effort for the thread, keeps approvals, tools, streaming,
interruption, and thread storage native, and shuts down its App Server child
when the TUI exits. `--resume` restores and preserves the native thread's stored
route rather than treating its next follow-up as a new task. If the opening
prompt has too little local evidence, cauto keeps Codex's incoming native model
and effort instead of forcing a generic guess.

The original `cauto "task"` form remains a one-shot launcher: it routes the
opening task and then replaces itself with native Codex on Unix. Use it when
session feedback and route pinning are not needed. An invocation without an
explicit prompt source fails in non-interactive preview/exec modes instead of
classifying an empty string as cheap work.

Prompt sources are positional text, `--prompt`, `--prompt-file`, or `--stdin`.
Use exactly one. Native arguments require `--`, so boundaries are unambiguous:

```bash
cauto --prompt "research upstream behavior" -- --search
cauto --prompt-file task.txt -- --image "screen one.png" --image screen-two.png
```

Everything after `--` is forwarded as the original `OsString` sequence.
The same delimiter works with agent mode:

```bash
cauto agent "inspect the live page" -- --search
```

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
policy accepts only `version` and `rules`; defaults, Fast, Ultra authorization,
downgrade policy, logging, cache/timeouts, hysteresis, and
weights remain user- or CLI-owned. Classifier keys from cauto 0.2 are rejected
with a migration message because routing is now local-only. Example user
configuration:

```toml
version = 1
default_model = "gpt-5.6-sol"
default_effort = "medium"
fast_mode = "inherit"
ultra_requires_opt_in = true
allow_automatic_downgrade = true
log_raw_prompts = false
catalog_cache_hours = 12
git_timeout_ms = 250
catalog_timeout_ms = 2500
hysteresis_points = 0

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

Prompt length is only a bounded scope hint: crossing 1,200 and 5,000 bytes can
add at most two scope levels, and length alone cannot reach High, Extra High,
Max, or Ultra. Natural-language failure symptoms, live operational repairs,
adversarial research, and routing-quality audits are scored from their task
semantics rather than their length.

Threshold hysteresis reads at most the last 256 KiB of redacted decision
history when `hysteresis_points` is explicitly configured above zero. It is off
by default in the one-shot launcher because separate invocations are separate
tasks, not turns in one identified thread. Preview decisions never affect hysteresis,
and history never contains a prior raw prompt.

Adaptive agent threads do not use cross-turn hysteresis because they do not
rerun the router between turns. The opening route stays pinned.
A clear underpowered or overkill correction is attached to that initial
decision and can influence a later new session only after the repository's
conservative calibration threshold is met. An explicit native model/effort
change intentionally replaces the current thread's pin.

`cauto models` shows the installed catalog. Add `--refresh`, `--bundled`,
`--include-hidden`, or `--json`. `cauto doctor` reports the resolved binary,
Codex version, paths, cache age/source, local routing engine, aliases, and
actual Max/Ultra support without printing secrets.

## First-Turn Latency And Route Notices

`cauto agent` loads repository context, typed config, compiled rules, and model
capabilities before the native input box appears. After Enter, the route is a
bounded in-memory calculation; no `codex exec`, network request, catalog load,
Git command, or filesystem walk is on that prompt path.

When local evidence is decisive, cauto applies the route and pins it. When it
is not, cauto preserves the native Codex route and records that provenance.
Clients that advertise the optional `infoNotifications` capability receive a
calm informational line such as `Route set · Luna / Low` or
`Native route kept · Sol / Extra High`, with a short reason. Older Codex builds
stay silent rather than rendering ordinary routing as a warning. Actual routing
or persistence failures still use native warning notifications.

## Privacy And History

Raw prompts are never stored. Decision records under
`~/.local/state/cauto/decisions.jsonl` contain a SHA-256 prompt digest, byte
length, bounded scores, rule IDs, selected capability, outcome category, and
sanitized argv with the prompt removed. Unknown `-c` values are redacted.
Cache/state directories and files use user-only permissions where supported.

```bash
cauto report
```

In agent mode, explicit native model/effort changes and clear conversational
corrections such as "still broken" or "that was overkill" are recorded as
feedback automatically. A repository needs at least three eligible signals and
70% agreement before cauto automatically applies a bounded +5 or -5 score-point
calibration for future sessions. The current thread remains on its pinned route
unless the user explicitly changes it.

Silence is not treated as approval. Prompt length, elapsed time, token use, and
tool count are not treated as outcome evidence. `cauto report` separates
adaptive-agent, direct launched, preview, and legacy/untyped decisions; its
primary route distribution and health rates use all real launches. It also
reports route-source distribution, native-preservation rate, local routing
p50/p95/max latency, feedback sources, and unresolved generic-baseline
concentration. Classifier rates are retained only for old schema-1 history and
are labeled as legacy.

## Automatic Adaptation And Optional Manual Controls

Routine agent use does not require `cauto feedback` or `cauto tune`. The old
commands remain available for compatibility, diagnostics, and deliberate
manual input:

```bash
cauto feedback right
cauto feedback overkill
cauto feedback underpowered
cauto feedback failed-for-other-reason
cauto tune
cauto tune --apply
```

`cauto tune` is still a read-only inspection unless `--apply` is supplied.
Agent-generated corrections use the same conservative threshold and apply an
eligible recommendation automatically. `right` counts against a change;
`failed-for-other-reason` is displayed but cannot affect routing. Preview
decisions and feedback attached to previews are excluded.
Native-preserved decisions are also excluded because cauto cannot learn from a
route it deliberately did not choose.

Applied values live separately from config and policy in
`~/.local/state/cauto/calibration.json`. The versioned file contains repository
identifiers and aggregate counts, never prompts. Writes are atomic and private.
Use `cauto tune --reset` to remove only the current (or `--repo PATH` selected)
repository's calibration. A later eligible agent signal may establish a new
bounded calibration.

Calibration is applied after deterministic feature and rule scoring and before
family/effort selection and hysteresis. Route output shows the configured and
effective offset separately from ordinary reasons. It cannot override explicit
model/family/effort choices, native overrides, project safety floors, catalog
checks, or Ultra authorization; it cannot escalate documentation/mechanical
work, manufacture Ultra eligibility, or force Max. Missing or malformed state
falls back to unchanged baseline routing. `cauto report` shows per-repository
eligibility, recommendations, applied calibration, and excluded previews.

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
planning, 1,000 prepared adaptive-agent first-turn routes, and pure scoring
while excluding final Codex runtime.
