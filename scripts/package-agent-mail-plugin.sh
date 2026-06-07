#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
host="$(rustc -vV | sed -n 's/^host: //p')"

if [[ -z "$host" ]]; then
  echo "could not determine Rust host target" >&2
  exit 1
fi

cargo build -p agent-mail --release

plugin_root="$repo_root/plugins/agent-mail"
bin_dir="$plugin_root/bin"
mkdir -p "$bin_dir"

# This script is the supported path for copying the host binary into the plugin
# cartridge. Generated agent-mail-* files are release artifacts.
install -m 0755 "$repo_root/target/release/agent-mail" "$bin_dir/agent-mail-$host"
chmod +x "$bin_dir/agent-mail"

for required in \
  "$plugin_root/.codex-plugin/plugin.json" \
  "$plugin_root/hooks/hooks.json" \
  "$repo_root/.agents/plugins/marketplace.json"; do
  if [[ ! -f "$required" ]]; then
    echo "missing required plugin file: $required" >&2
    exit 1
  fi
done

python3 - "$repo_root/.agents/plugins/marketplace.json" <<'PY'
import json
import sys

with open(sys.argv[1]) as handle:
    payload = json.load(handle)

for plugin in payload.get("plugins", []):
    if plugin.get("name") == "agent-mail":
        source = plugin.get("source", {})
        if source.get("source") == "local" and source.get("path") == "./plugins/agent-mail":
            break
else:
    raise SystemExit("marketplace.json must point agent-mail at ./plugins/agent-mail")
PY

echo "packaged plugins/agent-mail/bin/agent-mail-$host"
