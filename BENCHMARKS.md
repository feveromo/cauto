# Benchmarks

Measured 2026-07-16 on Linux 7.0.0-27-generic x86_64 with Rust 1.95.0,
`--release --locked`, thin LTO, one codegen unit, and stripped symbols.
`hyperfine` was unavailable, so the checked-in fallback used
`std::time::Instant` around child processes. The catalog was warm, the
routing engine was local-only, decision logging used an isolated state
directory, and final Codex runtime was excluded.

The synthetic project contained 100 rules, 500 phrases, and 100 path globs.
The normal prompt was about 80 bytes and the large prompt was 20,458 bytes.

| Operation | Iterations | Median | Mean | p95 |
| --- | ---: | ---: | ---: | ---: |
| `--help` | 100 | 0.377 ms | 0.394 ms | 0.436 ms |
| `explain` | 50 | 1.221 ms | 1.228 ms | 1.285 ms |
| warm deterministic `--dry-run` | 50 | 1.227 ms | 1.232 ms | 1.313 ms |
| 20 KB prompt route | 30 | 1.435 ms | 1.441 ms | 1.532 ms |
| cached `models --json` | 50 | 0.815 ms | 0.820 ms | 0.872 ms |
| route-to-command planning | 50 | 1.207 ms | 1.215 ms | 1.291 ms |

The adaptive first-turn benchmark prepares repository context, config, rules,
calibration, installation, and catalog once, then routes the exact project
explanation prompt 1,000 times through the same in-memory path used after Enter.
With the synthetic 100-rule policy it measured 9.350 us median, 9.868 us mean,
13.750 us p95, and 28.881 us max.

Core microbenchmarks:

| Operation | Mean per operation |
| --- | ---: |
| Typed config parse and merge | 157.053 us |
| Compile 100 rules | 306.603 us |
| Phrase plus path match | 251 ns |
| Catalog envelope deserialize | 5.361 us |
| Decision JSON serialization | 643 ns |
| Weighted score calculation | 2 ns |
| Full pure route calculation | 58 ns |

The release binary was 3,484,592 bytes. These numbers are host measurements,
not universal guarantees. Reproduce them with `scripts/bench.sh`.
