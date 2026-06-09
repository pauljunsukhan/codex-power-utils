#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
host="$(rustc -vV | sed -n 's/^host: //p')"
plugin_root="$repo_root/plugins/agent-mail"
bin_dir="$plugin_root/bin"
mcp_config="$plugin_root/.mcp.json"

if [[ -z "$host" ]]; then
  echo "could not determine Rust host target" >&2
  exit 1
fi

for required in \
  "$plugin_root/.codex-plugin/plugin.json" \
  "$mcp_config" \
  "$plugin_root/hooks/hooks.json" \
  "$plugin_root/bin/agent-mail" \
  "$repo_root/.agents/plugins/marketplace.json"; do
  if [[ ! -f "$required" ]]; then
    echo "missing required plugin file: $required" >&2
    exit 1
  fi
done

python3 - "$repo_root/.agents/plugins/marketplace.json" "$mcp_config" <<'PY'
import json
import sys
from pathlib import Path

with open(sys.argv[1]) as handle:
    payload = json.load(handle)

for plugin in payload.get("plugins", []):
    if plugin.get("name") == "agent-mail":
        source = plugin.get("source", {})
        if source.get("source") == "local" and source.get("path") == "./plugins/agent-mail":
            break
else:
    raise SystemExit("marketplace.json must point agent-mail at ./plugins/agent-mail")

manifest_path = Path(sys.argv[2]).parent / ".codex-plugin" / "plugin.json"
with open(manifest_path) as handle:
    manifest = json.load(handle)

if "hooks" in manifest:
    raise SystemExit("plugin.json must not declare unsupported field hooks; use hooks/hooks.json")

if manifest.get("mcpServers") != "./.mcp.json":
    raise SystemExit("plugin.json mcpServers must be ./.mcp.json")

with open(sys.argv[2]) as handle:
    mcp = json.load(handle)

server = mcp.get("mcpServers", {}).get("agent_mail")
if not isinstance(server, dict):
    raise SystemExit(".mcp.json must define mcpServers.agent_mail")

if server.get("command") != "./bin/agent-mail":
    raise SystemExit(".mcp.json mcpServers.agent_mail.command must be ./bin/agent-mail")

args = server.get("args")
if not isinstance(args, list) or args[:1] != ["serve-mcp"]:
    raise SystemExit(".mcp.json mcpServers.agent_mail.args must start with serve-mcp")

if server.get("cwd") != ".":
    raise SystemExit(".mcp.json mcpServers.agent_mail.cwd must be .")
PY

cargo build -p agent-mail --release

mkdir -p "$bin_dir"

# This script copies the host binary into the plugin cartridge for local install
# and smoke testing. Generated agent-mail-* files are ignored by git.
install -m 0755 "$repo_root/target/release/agent-mail" "$bin_dir/agent-mail-$host"
chmod +x "$bin_dir/agent-mail"

echo "packaged plugins/agent-mail/bin/agent-mail-$host"
