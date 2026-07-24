#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build --release --locked
binary="$root/target/release/cauto"
work="$(mktemp -d "${TMPDIR:-/tmp}/cauto-bench.XXXXXX")"
trap 'rm -rf "$work"' EXIT

original_home="${HOME:?cauto benchmark: HOME is required}"
if [[ -z "${CODEX_HOME+x}" ]]; then
  export CODEX_HOME="$original_home/.codex"
fi
export HOME="$work/home"
export XDG_CONFIG_HOME="$work/config"
export XDG_CACHE_HOME="$work/cache"
export XDG_STATE_HOME="$work/state"
export CAUTO_DISABLE_LOG=1
mkdir -p "$HOME"

case "$(uname -s)" in
  Darwin) catalog_dir="$HOME/Library/Caches/cauto/catalogs" ;;
  *) catalog_dir="$XDG_CACHE_HOME/cauto/catalogs" ;;
esac

repo="$work/repo"
mkdir -p "$repo"
policy="$repo/.cauto.toml"
printf 'version = 1\n' > "$policy"
for ((index = 1; index <= 100; index++)); do
  printf '\n[[rules]]\n' >> "$policy"
  printf 'id = "bench-%03d"\n' "$index" >> "$policy"
  printf 'phrases = ["benchmark phrase %03d a", "benchmark phrase %03d b", "benchmark phrase %03d c", "benchmark phrase %03d d", "benchmark phrase %03d e"]\n' \
    "$index" "$index" "$index" "$index" "$index" >> "$policy"
  printf 'path_globs = ["src/module%03d/**"]\n' "$index" >> "$policy"
  printf 'dimension_deltas = { scope = 1 }\n' >> "$policy"
  printf 'confidence_delta = 0.001\n' >> "$policy"
  printf 'reason = "Synthetic benchmark evidence."\n' >> "$policy"
done

typical_prompt="benchmark phrase 050 c update src/module050/file.rs with exact expected behavior"
large_prompt_file="$work/large-prompt.txt"
awk 'BEGIN { for (i = 0; i < 386; i++) printf "benchmark phrase %03d a inspect src/module%03d/file.rs ", (i % 100) + 1, (i % 100) + 1 }' > "$large_prompt_file"

"$binary" --repo "$repo" --dry-run "$typical_prompt" >/dev/null
"$binary" --repo "$repo" --json models >/dev/null

export CAUTO_BENCH_BINARY="$binary"
export CAUTO_BENCH_REPO="$repo"
export CAUTO_BENCH_PROMPT="$typical_prompt"
export CAUTO_BENCH_LARGE_PROMPT_FILE="$large_prompt_file"
help_command='"$CAUTO_BENCH_BINARY" --help >/dev/null'
explain_command='"$CAUTO_BENCH_BINARY" --repo "$CAUTO_BENCH_REPO" explain "$CAUTO_BENCH_PROMPT" >/dev/null'
dry_command='"$CAUTO_BENCH_BINARY" --repo "$CAUTO_BENCH_REPO" --dry-run "$CAUTO_BENCH_PROMPT" >/dev/null'
large_command='"$CAUTO_BENCH_BINARY" --repo "$CAUTO_BENCH_REPO" --dry-run --prompt-file "$CAUTO_BENCH_LARGE_PROMPT_FILE" >/dev/null'
catalog_command='"$CAUTO_BENCH_BINARY" --repo "$CAUTO_BENCH_REPO" --json models >/dev/null'
route_exec_command='"$CAUTO_BENCH_BINARY" --repo "$CAUTO_BENCH_REPO" --dry-run --print-command "$CAUTO_BENCH_PROMPT" >/dev/null'

echo "Final Codex runtime is excluded from every timed route."
if command -v hyperfine >/dev/null 2>&1; then
  hyperfine --warmup 10 --runs 50 \
    --command-name help "$help_command" \
    --command-name explain "$explain_command" \
    --command-name dry-run "$dry_command" \
    --command-name large-prompt "$large_command" \
    --command-name cached-catalog "$catalog_command" \
    --command-name route-to-command "$route_exec_command"
else
  echo "hyperfine unavailable; using cauto's std::time::Instant child timer."
  measure() {
    local name="$1"
    local iterations="$2"
    shift 2
    printf '%-18s ' "$name"
    "$binary" bench-process --iterations "$iterations" "$binary" -- "$@"
  }
  measure help 100 --help
  measure explain 50 --repo "$repo" explain "$typical_prompt"
  measure dry-run 50 --repo "$repo" --dry-run "$typical_prompt"
  measure large-prompt 30 --repo "$repo" --dry-run --prompt-file "$large_prompt_file"
  measure cached-catalog 50 --repo "$repo" --json models
  measure route-to-command 50 --repo "$repo" --dry-run --print-command "$typical_prompt"
fi

catalog_file=""
for candidate in "$catalog_dir"/*.json; do
  if [[ -f "$candidate" ]]; then
    catalog_file="$candidate"
    break
  fi
done
if [[ -z "$catalog_file" ]]; then
  echo "cauto benchmark: no catalog cache was created under $catalog_dir" >&2
  exit 1
fi

"$binary" bench-core --policy "$policy" --catalog "$catalog_file" --iterations 1000
"$binary" --repo "$repo" bench-agent-route --iterations 1000
"$binary" bench-score --iterations 10000000
if stat -c '%s' "$binary" >/dev/null 2>&1; then
  echo "release_binary_bytes=$(stat -c '%s' "$binary")"
else
  echo "release_binary_bytes=$(wc -c < "$binary" | tr -d ' ')"
fi
