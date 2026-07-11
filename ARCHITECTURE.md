# Architecture

## Data Flow

`main.rs` parses Clap input, calls the library entrypoint, renders one typed
error, and returns its stable exit code. The library performs:

```text
prompt argv
  -> repository root + bounded Git/AGENTS metadata
  -> typed user/project config merge
  -> one normalized task buffer
  -> compiled phrase/path policy evidence
  -> pure fixed-point route
  -> optional isolated classifier blend
  -> fingerprinted installed-catalog resolution
  -> redacted locked decision append
  -> argv LaunchPlan
  -> native Unix exec
```

No repository recursion, model inference, network client, async runtime, or
Codex subprocess is present on a warm deterministic `--no-classifier` route.
Git is the only optional pre-launch subprocess and is bounded by one command.

## Configuration And Policy

External TOML deserializes into `RawConfig`. Each layer is validated with its
source path before typed field-by-field merging into `ValidatedConfig`.
Precedence is cauto CLI, explicit native model/reasoning/tier, project config,
user config, router, then conservative defaults. Duplicate project rule IDs
replace lower-layer IDs. Contradictory bounds inside one rule are rejected;
conflicts between independently matched rules remain visible and reduce
confidence.

Rules are metadata only. Aho-Corasick maps phrase patterns back to typed rules,
and GlobSet maps mentioned paths back to rules. Each matched rule applies at
most once.

## Pure Router

`routing` owns validated `BoundedScore`, `Confidence`, dimensions, evidence,
floors/ceilings, fixed integer weighting, confidence, hysteresis, family choice,
effort choice, and Ultra eligibility. It has no filesystem or process I/O.
Risk dimensions are monotonic absent an explicit ceiling. Parallelizability
does not raise normal complexity. Application orchestration supplies the latest
same-repository effort from a bounded decision-log tail so the pure selector's
threshold hysteresis is effective without an unbounded history read.

Classifier output is a 30% evidence blend after strict range and length checks.
Deterministic policy remains 70%, and the same floors, ceilings, and Ultra gate
are reapplied.

## Catalog And Cache

Codex discovery resolves explicit `--codex-bin`, `CODEX_BIN`, then PATH and
rejects the running cauto executable. Its fingerprint hashes canonical path,
length, mtime, Unix device/inode, `CODEX_HOME`, profile identity, and distinct
`codex`/`codex-openai` PATH entrypoints so thin wrappers invalidate when their
underlying installed package changes.

Catalog adapters are isolated behind `CatalogSource`: digest-checked cache,
`codex debug models`, bundled debug models, an explicit future App Server
adapter boundary, and a conservative built-in fallback. Additive catalog fields
are ignored. The fallback knows Sol/Terra/Luna IDs but never claims Max or
Ultra.

Cache envelopes include schema/cauto/Codex versions, fingerprint, `CODEX_HOME`
hash, profile, timestamps, source, payload SHA-256, and catalog. Valid stale
data is used without a warm refresh. Missing or explicit refreshes share a
bounded refresh lock. Writes use a same-directory unique temporary file,
flush, data sync, atomic rename, and restrictive permissions.

Max and Ultra are internal capability requests until a selected model exposes
literal `max` or `ultra` through the installed catalog. Unsupported automatic
routes downgrade visibly. Unsupported explicit routes fail unless
`--allow-downgrade` is present.

## Classifier Boundary

The optional Luna classifier runs only through native `codex exec`; it is not
an API client. The child receives `CAUTO_CLASSIFIER=1`, uses a fresh private
directory, read-only sandbox, ephemeral session, low effort, schema path, and
bounded prompt metadata. Unix assigns a process group so timeout terminates and
reaps descendants. Temporary files are removed on every return path.

## Launch Boundary

Forwarded arguments remain `OsString` values. Inspection recognizes only
specific native model/profile/config keys and never rewrites argv. The
`LaunchPlan` keeps inherited and injected args separate. Only resolved model,
effort, explicitly selected service tier, working directory, and cauto profile
may be injected; approval, sandbox, network, provider, auth, MCP, rules, hooks,
and inherited Fast state are untouched.

Before launch, output and the redacted JSONL record are flushed and locks are
released. Unix calls `CommandExt::exec`, so there is no wrapper parent.
Non-Unix uses inherited stdio and propagates the child status.

## Phase 2

`cauto agent` should be a separate App Server client after decision/feedback
data justifies it. It must negotiate initialize, `model/list`,
`collaborationMode/list`, provider capabilities, and experimental features;
handle JSON-RPC IDs, notifications, streaming deltas, approvals, user input,
resize, interruption, cancellation, and thread persistence; and change routes
only between completed turns. It must preserve sandbox/approval policy and make
every transition visible. None of that lifecycle is mixed into version 1.
