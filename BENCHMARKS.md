# Benchmarks

Measured 2026-07-11 on Linux 7.0.0-27-generic x86_64 with Rust 1.95.0,
`--release --locked`, thin LTO, one codegen unit, and stripped symbols.
`hyperfine` was unavailable, so the checked-in fallback used
`std::time::Instant` around child processes. The catalog was warm, the
classifier was disabled, decision logging used an isolated state directory, and
final Codex runtime was excluded.

The synthetic project contained 100 rules, 500 phrases, and 100 path globs.
The normal prompt was about 80 bytes and the large prompt was 20,458 bytes.

| Operation | Iterations | Median | Mean | p95 |
| --- | ---: | ---: | ---: | ---: |
| `--help` | 100 | 0.367 ms | 0.373 ms | 0.412 ms |
| `explain` | 50 | 1.187 ms | 1.192 ms | 1.261 ms |
| warm deterministic `--dry-run` | 50 | 1.179 ms | 1.186 ms | 1.262 ms |
| 20 KB prompt route | 30 | 1.307 ms | 1.314 ms | 1.371 ms |
| cached `models --json` | 50 | 0.804 ms | 0.803 ms | 0.846 ms |
| route-to-command planning | 50 | 1.183 ms | 1.179 ms | 1.226 ms |

Core microbenchmarks:

| Operation | Mean per operation |
| --- | ---: |
| Typed config parse and merge | 155.382 us |
| Compile 100 rules | 292.199 us |
| Phrase plus path match | 246 ns |
| Catalog envelope deserialize | 5.524 us |
| Decision JSON serialization | 538 ns |
| Weighted score calculation | 2 ns |
| Full pure route calculation | 58 ns |

The release binary was 2,963,280 bytes. These numbers are host measurements,
not universal guarantees. Reproduce them with `scripts/bench.sh`.
