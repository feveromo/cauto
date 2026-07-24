# Routing Guide

`cauto` is a local decision layer in front of the native Codex CLI. It does not
ask another model which model to use. For each opening task it:

1. extracts bounded evidence from the prompt,
2. refines that evidence for task shape, breadth, and correlated risk,
3. applies explicit low-cost budgets for narrowly defined work,
4. applies user and repository policy rules,
5. computes a deterministic score,
6. applies safety floors that a weighted average is not allowed to hide,
7. resolves the recommendation against the models and efforts actually installed.

Use `cauto explain "..."` to inspect a decision without launching Codex. The
output includes the inferred task type, score, confidence, strongest reasons,
and all seven dimensions.

## Default behavior

| Task shape | Typical route |
| --- | --- |
| Typo, wording edit, narrow rename | Luna / Low |
| Simple whole-project explanation | Luna / Low |
| Precise one-file implementation | Terra / Medium |
| Open-ended repository improvement | Sol / High |
| Cross-platform runtime change | Sol / High |
| Security, state migration, or concurrency work | Sol / High or stronger |
| Architecture, reverse engineering, or live diagnosis | Sol / High or stronger |

These are defaults, not aliases for exact model IDs. The live Codex catalog
decides which installed model represents Luna, Terra, or Sol and which native
reasoning efforts are actually available.

## Why the router has both a score and floors

A weighted average is useful for ordinary work, but it can dilute one dangerous
dimension. A small credential change, race-condition repair, or state migration
may have limited scope while still carrying a high cost of being wrong. The
semantic guard therefore enforces a minimum effort for correlated high-risk
evidence even when the raw score would otherwise land in Medium.

Explicit user choices still win. Policy ceilings remain respected, and Ultra
still requires both real independent workstreams and explicit delegation
authorization.

## Calibration

Repository calibration is deliberately small and bounded. It can nudge future
sessions after repeated, consistent feedback, but it cannot manufacture Max or
Ultra, bypass explicit choices, or erase the risk floors described above.
