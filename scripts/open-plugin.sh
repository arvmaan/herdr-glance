#!/bin/sh
set -eu

root="${HERDR_PLUGIN_ROOT:-$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)}"
binary="$root/target/release/herdr-glance-app"

if [ ! -x "$binary" ]; then
  echo "Glance is not built. Reinstall the plugin to run its build step." >&2
  exit 1
fi

nohup "$binary" >/dev/null 2>&1 &
