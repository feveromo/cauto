#!/usr/bin/env bash
set -euo pipefail

dry_run=false
skip_tests=false

for argument in "$@"; do
  case "$argument" in
    --dry-run) dry_run=true ;;
    --skip-tests) skip_tests=true ;;
    *)
      echo "usage: scripts/install.sh [--dry-run] [--skip-tests]" >&2
      exit 2
      ;;
  esac
done

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

command -v cargo >/dev/null || {
  echo "cauto installer: cargo is required" >&2
  exit 1
}
command -v rustc >/dev/null || {
  echo "cauto installer: rustc is required" >&2
  exit 1
}

rust_version="$(rustc --version | awk '{print $2}')"
if ! awk -v version="$rust_version" 'BEGIN {
  split(version, got, ".");
  if (got[1] > 1 || (got[1] == 1 && got[2] >= 89)) exit 0;
  exit 1;
}'; then
  echo "cauto installer: rustc 1.89 or newer is required (found $rust_version)" >&2
  exit 1
fi

run() {
  if "$dry_run"; then
    printf '+'
    printf ' %q' "$@"
    printf '\n'
  else
    "$@"
  fi
}

if ! "$skip_tests"; then
  run cargo fmt --check
  run cargo clippy --all-targets --all-features -- -D warnings
  run cargo test --all-targets --all-features
fi

run cargo install --path . --locked --force

if "$dry_run"; then
  printf '%s\n' '+ command -v cauto' '+ cauto --version' '+ cauto doctor'
else
  command -v cauto
  cauto --version
  cauto doctor
fi
