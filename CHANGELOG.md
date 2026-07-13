# Changelog

## Unreleased

## 0.2.0 - 2026-07-13

- Add `cauto agent`, a loopback-only transparent App Server transport that
  routes every native Codex TUI text turn while preserving native lifecycle,
  approvals, tools, streaming, interruption, and persisted threads.
- Add thread-local hysteresis, visible route transitions, resume-state seeding,
  one-step correction/overkill adaptation, and temporary repeated-failure
  escalation without using weak outcome proxies.
- Record explicit route changes and clear conversational corrections
  automatically, and auto-apply the existing bounded repository calibration
  only after three signals with at least 70% directional agreement.
- Count adaptive-agent turns as real launches in reports and expose agent route
  and feedback-source distributions.

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
