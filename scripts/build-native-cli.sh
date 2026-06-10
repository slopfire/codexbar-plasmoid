#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
crate_dir="$repo_root/native-cli"
output_binary="$repo_root/plasmoid/contents/code/codexbar-plasmoid"

if ! command -v cargo >/dev/null 2>&1; then
  echo "cargo is required to build codexbar-plasmoid." >&2
  exit 1
fi

cargo build --release --manifest-path "$crate_dir/Cargo.toml"
install -m 755 "$crate_dir/target/release/codexbar-plasmoid" "$output_binary"
echo "Built $output_binary"
