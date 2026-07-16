# Architecture

## Data Flow

`main.rs` parses Clap input, calls the library entrypoint, renders one typed
error, and returns its stable exit code. The one-shot launcher performs:

```text
prompt argv
  -> repository root + bounded Git/AGENTS metadata
  -> typed user/project config merge
  -> one normalized task buffer
  -> compiled phrase/path policy evidence
  -> optional bounded repository calibration value
  -> pure fixed-point route
  -> fingerprinted installed-catalog resolution
  -> redacted locked decision append
  -> argv LaunchPlan
  -> native Unix exec
```

No repository recursion, model inference, network client, or async runtime is
present in routing. Git is the only optional pre-launch subprocess and is
bounded by one command.

The adaptive path keeps the same routing pipeline but places it inside a
loopback WebSocket relay:

```text
native Codex TUI
  <-> cauto transparent relay (intercepts turn/start only)
  <-> native Codex App Server
```

The relay may rewrite the selected model/effort at the first text turn boundary,
pins that route for the thread, and forwards every other request, response,
notification, stream frame, approval, input, resize, interrupt, cancel, ping,
and close frame unchanged.

## Configuration And Policy

User TOML deserializes into `RawConfig`; repository TOML deserializes into the
separate `ProjectPolicy`, which accepts only `version` and `rules`. Each layer
is validated with its source path before project rules are merged into
`ValidatedConfig`. Precedence is cauto CLI, explicit native
model/reasoning/tier, project rules, user config, router, then conservative
defaults. Duplicate project rule IDs replace lower-layer IDs. Contradictory
bounds inside one rule are rejected; conflicts between independently matched
rules remain visible and reduce confidence.

Rules are metadata only. Aho-Corasick maps phrase patterns back to typed rules,
and GlobSet maps mentioned paths back to rules. Each matched rule applies at
most once.

## Pure Router

`routing` owns validated `BoundedScore`, `Confidence`, dimensions, evidence,
floors/ceilings, fixed integer weighting, `ScoreCalibration`, confidence,
hysteresis, family choice, effort choice, and Ultra eligibility. It has no
filesystem or process I/O.
Risk dimensions are monotonic absent an explicit ceiling. Parallelizability
does not raise normal complexity. Optional threshold hysteresis can read the
latest same-repository route from a bounded decision-log tail, but it is off by
default: the one-shot path has no thread identity, so two launcher invocations
in one repository must be treated as independent tasks rather than adjacent turns.
Agent mode routes only the opening text turn and pins the selected model,
reasoning effort, and service tier for the thread. It therefore does not feed
follow-up turns through hysteresis. Resume responses seed a stored route and
prevent the next follow-up from being treated as a new task.

Application orchestration also loads an optional repository-hash calibration
and passes the validated -10 through +10 typed value into selection. The router
applies it after deterministic scoring and before hysteresis, and returns a
separate `CalibrationEffect`. Upward offsets are suppressed for documentation
and mechanical work and capped below the Max threshold; Ultra eligibility uses
the uncalibrated score. Explicit choices and policy constraints are applied at
their normal higher-authority boundaries. Preview decisions are excluded from
both prior-route lookup and tuning evidence.

Application orchestration marks whether the local route has decisive evidence.
Typed task recognition, escalation signals, explicit paths or completion
criteria, matched policy, and explicit model/effort choices are decisive. An
unrecognized generic prompt is not: adaptive mode preserves the incoming native
Codex route instead of allowing the generic baseline to masquerade as evidence.

## Catalog And Cache

Codex discovery resolves explicit `--codex-bin`, `CODEX_BIN`, then PATH and
rejects the running cauto executable. Its fingerprint hashes canonical path,
length, mtime, Unix device/inode, `CODEX_HOME`, profile identity, and distinct
`codex`/`codex-openai` PATH entrypoints so thin wrappers invalidate when their
underlying installed package changes.

Catalog adapters are isolated behind `CatalogSource`: digest-checked cache,
`codex debug models`, bundled debug models, and a conservative built-in
fallback. Agent startup separately negotiates the live App Server model list
before opening the TUI. Additive catalog fields are ignored. The fallback knows
Sol/Terra/Luna IDs but never claims Max or Ultra.

Cache envelopes include schema/cauto/Codex versions, fingerprint, `CODEX_HOME`
hash, profile, timestamps, source, payload SHA-256, and catalog. Fresh cache
returns immediately. Missing, stale, and explicit refreshes share a bounded
refresh lock; a failed stale refresh returns the prior catalog with a warning.
Writes use a same-directory unique temporary file, flush, data sync, atomic
rename, and restrictive permissions.

Max and Ultra are internal capability requests until a selected model exposes
literal `max` or `ultra` through the installed catalog. Unsupported automatic
routes downgrade visibly. Unsupported explicit routes fail unless
`--allow-downgrade` is present.

## Prepared First-Turn Boundary

Adaptive startup resolves repository and AGENTS metadata, typed configuration,
calibration, Codex installation, model capabilities, and compiled policy before
the TUI accepts a prompt. The live App Server catalog replaces the prepared
fallback/cache catalog during negotiation. A first `turn/start` therefore does
only prompt normalization, local feature extraction, compiled-rule evaluation,
fixed-point selection, capability lookup, and JSON field rewriting in memory.
No child process, Git query, filesystem scan, or network operation begins after
Enter. Later turns reuse the pin without running even that local pipeline.

## Feedback And Calibration State

Decision and feedback JSONL remains append-only and redacted. Tuning analysis
joins feedback to its decision ID, excludes preview-linked and native-preserved
events, ignores diagnostic failures for eligibility, requires three routing
outcomes, and requires a 70% directional signal. The recommendation is a conservative target
offset of +5 or -5 points. Manual analysis remains read-only until
`cauto tune --apply`; adaptive agent corrections apply an eligible
recommendation automatically.

Agent feedback accepts only explicit model/effort route changes and
high-precision correction/overkill language. It never infers success from
silence, duration, tokens, prompt length, or tool count. Feedback can update
bounded repository calibration for later sessions, but it never reroutes the
current thread. A concentration of locally launched decisions at the unresolved
generic baseline is reported directly and should trigger a deterministic
feature or policy correction rather than repetitive user feedback.

Applied calibration uses a separate versioned
`~/.local/state/cauto/calibration.json` store keyed only by repository hash.
Entries contain the bounded offset, direction, aggregate counts, and timestamp;
they contain no prompts. Atomic same-directory replacement and user-only modes
match cache/state durability rules. Parsing failure is non-fatal on the launch
path and yields baseline routing. `cauto tune --reset` removes only the selected
repository entry.

## Launch Boundary

Forwarded arguments remain `OsString` values. Inspection recognizes only
specific native model/profile/config keys and never rewrites argv. The
`LaunchPlan` keeps inherited and injected args separate. Only resolved model,
effort, explicitly selected service tier, working directory, and cauto profile
may be injected; approval, sandbox, network, provider, auth, MCP, rules, hooks,
and inherited Fast state are untouched.

Before a one-shot launch, output and the redacted JSONL record are flushed and
locks are released. Unix calls `CommandExt::exec`, so there is no wrapper
parent. Non-Unix uses inherited stdio and propagates the child status.

## Adaptive App Server Boundary

`cauto agent` starts a native `codex app-server` on an ephemeral loopback
endpoint, negotiates `initialize`, `model/list`, `collaborationMode/list`,
provider capabilities, and experimental features, then launches native Codex
with `--remote` pointed at cauto's separately reserved loopback endpoint. The
control connection closes after negotiation; the TUI connection remains a
transparent full-duplex relay.

Only the first client `turn/start` text request for a new thread is routed and
logged, after App Server accepts it. Both top-level model/effort and
authoritative `collaborationMode.settings` are rewritten, then the exact model,
native effort, and service tier are pinned for later turns without invoking the
router again. A later native turn setting that differs from the
pin is treated as an explicit user route change and becomes the new pin;
`thread/resume` responses restore and pin the stored route. Clear corrections
are feedback for future sessions, not triggers for a same-thread route change.
The real request/response is relayed before cosmetic routing messages or
decision logging. If local evidence is insufficient, the untouched native route
is pinned. If routing fails, the untouched native request is forwarded with a
warning instead of blocking the session. Successful route choices are emitted
only as an `info` notification when the TUI advertises support; older clients
remain silent, while actual failures retain warning severity.

Child guards terminate and reap the App Server and TUI on every return path.
The relay binds only `127.0.0.1`, inherits the native TUI's stdio, sandbox,
approval, auth, profile, MCP, skills, hooks, and provider behavior, and contains
no OpenAI API key or billing path.

The proxy listener is nonblocking only while it waits for the TUI to connect.
It keeps waiting as long as the native TUI process is alive, allowing Codex to
complete interactive update or authentication prompts before opening the remote
connection. Once connected, the accepted stream is restored to blocking mode
before its WebSocket handshake; bounded read timeouts drive cooperative duplex
polling while writes retain normal backpressure. This keeps a transient Darwin
`EAGAIN` from terminating the relay during bursty App Server startup events.
