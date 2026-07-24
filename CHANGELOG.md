# Changelog

## Unreleased

- Surface every successful first-turn route by default, including its reason
  and thread pin state. Use native informational notices when supported and a
  compatible visible fallback otherwise; `--quiet` suppresses success notices
  without hiding routing or persistence failures.
- Keep bounded helper output draining after its capture limit, isolate helper
  subprocesses into Unix process groups, and clean up descendants after parent
  exit, timeout, or wait failure so they cannot stall catalog or version
  discovery.
- Stop App Server connection retries as soon as the child exits, monitor it
  while the TUI completes preflight, and preserve conventional signal-derived
  TUI exit codes.
- Reject non-directory cache/state paths without modifying them, tighten
  existing decision logs to user-only permissions, and keep installer
  validation locked to the same dependency graph as CI.
- Make benchmark isolation, fixture generation, and command quoting portable
  across Linux and macOS, with shell-script checks on both CI runners.

## 0.3.0 - 2026-07-16

- Remove the hidden model classifier and all classifier CLI/config controls;
  prompt submission now performs local Rust routing only.
- Prepare repository context, config, compiled rules, calibration, and model
  capabilities before the adaptive TUI accepts input, leaving only bounded
  in-memory work on the first-turn hot path.
- Preserve Codex's native first-turn model and effort when local evidence is
  insufficient, then pin that route without treating it as calibration data.
- Recognize project/codebase explanation requests locally and route them to
  Luna/Low without a second model call.
- Add route provenance and p50/p95/max routing latency to decision history and
  reports; retain classifier rates only as explicitly labeled legacy metrics.
- Negotiate the complete live App Server model catalog and use it for adaptive
  route capability checks.
- Add a capability-gated `info` notification for calm route confirmations;
  older Codex clients remain silent and real failures continue to warn.
- Relay real App Server requests and responses before cosmetic route notices or
  decision logging.

- Restore blocking mode on accepted adaptive-agent TUI sockets before the
  WebSocket handshake, preventing transient macOS `EAGAIN` failures during MCP
  startup and allowing Codex to restore the terminal cleanly on exit.
- Allow the native TUI to finish interactive update or authentication prompts
  before it connects to the adaptive-agent relay instead of terminating it
  after ten seconds.
- Run formatting, Clippy, and the complete test suite on Linux and macOS in CI.

## 0.2.0 - 2026-07-13

- Add `cauto agent`, a loopback-only transparent App Server transport that
  routes the opening native Codex TUI text turn, pins that route for the thread,
  and preserves native lifecycle, approvals, tools, streaming, interruption,
  and persisted threads.
- Preserve resumed routes and honor explicit in-session model changes without
  treating ordinary follow-up prompts as new routing decisions.
- Record explicit route changes and clear conversational corrections
  automatically, and auto-apply the existing bounded repository calibration
  for future sessions only after three signals with at least 70% directional
  agreement.
- Count adaptive-agent session decisions as real launches in reports and expose
  agent route and feedback-source distributions.

- Route natural-language operational failures, routing audits, and adversarial
  research from task risk instead of prompt length or a generic baseline.
- Use the isolated Luna classifier automatically only for low-confidence tasks
  with no deterministic semantic evidence; classifier evidence can raise but
  never erase deterministic risk.
- Disable repository-global hysteresis by default until routing has real thread
  identity.
- Separate launched, preview, and legacy/untyped decisions in reports and
  surface generic-baseline concentration directly.

## 0.1.0 - 2026-07-11

- Initial repository-aware deterministic router, native Codex launcher,
  capability cache, optional Luna classifier, and redacted decision history.
