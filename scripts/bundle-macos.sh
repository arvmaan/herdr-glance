#!/bin/bash
set -euo pipefail

script_dir="$(cd "$(dirname "$0")" && pwd)"
project_dir="$(dirname "$script_dir")"
app="$project_dir/target/release/bundle/macos/Herdr Glance.app"

cd "$project_dir/crates/herdr-glance-app"
cargo tauri icon ../../assets/icon.png
cargo tauri build
codesign --force --deep --sign - "$app"

echo "Built: $app"

if [[ "${1:-}" == "--install" ]]; then
  rm -rf "/Applications/Herdr Pills.app"
  rm -rf "/Applications/Herdr Glance.app"
  cp -R "$app" /Applications/
  open "/Applications/Herdr Glance.app"
fi
