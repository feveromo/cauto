#!/usr/bin/env bash
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

cargo build --release --locked
binary="$root/target/release/cauto"
work="$(mktemp -d "${TMPDIR:-/tmp}/cauto-bench.XXXXXX")"
trap 'rm -rf "$work"' EXIT

export XDG_CONFIG_HOME="$work/config"
export XDG_CACHE_HOME="$work/cache"
export XDG_STATE_HOME="$work/state"
export CAUTO_DISABLE_LOG=1

repo="$work/repo"
mkdir -p "$repo"
policy="$repo/.cauto.toml"
printf 'version = 1\n' > "$policy"
for index in $(seq 1 100); do
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

"$binary" --repo "$repo" --no-classifier --dry-run "$typical_prompt" >/dev/null
"$binary" --repo "$repo" --json models >/dev/null

help_command="$binary --help >/dev/null"
explain_command="$binary --repo $repo explain --no-classifier '$typical_prompt' >/dev/null"
dry_command="$binary --repo $repo --no-classifier --dry-run '$typical_prompt' >/dev/null"
large_command="$binary --repo $repo --no-classifier --dry-run --prompt-file $large_prompt_file >/dev/null"
catalog_command="$binary --repo $repo --json models >/dev/null"
route_exec_command="$binary --repo $repo --no-classifier --dry-run --print-command '$typical_prompt' >/dev/null"

echo "Classifier and final Codex runtime are excluded from every timed route."
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
  measure explain 50 --repo "$repo" explain --no-classifier "$typical_prompt"
  measure dry-run 50 --repo "$repo" --no-classifier --dry-run "$typical_prompt"
  measure large-prompt 30 --repo "$repo" --no-classifier --dry-run --prompt-file "$large_prompt_file"
  measure cached-catalog 50 --repo "$repo" --json models
  measure route-to-command 50 --repo "$repo" --no-classifier --dry-run --print-command "$typical_prompt"
fi

catalog_file="$(find "$XDG_CACHE_HOME/cauto/catalogs" -type f -name '*.json' | head -n 1)"
"$binary" bench-core --policy "$policy" --catalog "$catalog_file" --iterations 1000
"$binary" bench-score --iterations 10000000
if stat -c '%s' "$binary" >/dev/null 2>&1; then
  echo "release_binary_bytes=$(stat -c '%s' "$binary")"
else
  echo "release_binary_bytes=$(wc -c < "$binary" | tr -d ' ')"
fi
