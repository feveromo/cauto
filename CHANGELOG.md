# Changelog

## Unreleased

- Restore blocking mode on accepted adaptive-agent TUI sockets before the
  WebSocket handshake, preventing transient macOS `EAGAIN` failures during MCP
  startup and allowing Codex to restore the terminal cleanly on exit.
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
