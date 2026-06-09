#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wrapper="$repo_root/plugins/agent-mail/bin/agent-mail"
marketplace="$repo_root/.agents/plugins/marketplace.json"
plugin="$repo_root/plugins/agent-mail"
host="$(rustc -vV | sed -n 's/^host: //p')"
packaged_binary="$plugin/bin/agent-mail-$host"
cleanup_packaged_binary=0
tmp_home=""
expected_main_reminder='You have Agent Mail for coordinating with Codex agents. Role: main agent.'
expected_subagent_reminder='You have Agent Mail context from your main agent. Role: subagent. Do not initiate Agent Mail with `agent_mail.write`'
expected_mcp_instructions='Agent Mail is a stateless adapter over Codex thread APIs.'

if [[ -z "$host" ]]; then
  echo "could not determine Rust host target" >&2
  exit 1
fi

if [[ ! -e "$packaged_binary" ]]; then
  cleanup_packaged_binary=1
fi

cleanup() {
  if [[ "$cleanup_packaged_binary" == "1" ]]; then
    rm -f "$packaged_binary"
  fi
  if [[ -n "${tmp_home:-}" ]]; then
    rm -rf "$tmp_home"
  fi
}
trap cleanup EXIT

if ! package_output="$("$repo_root/scripts/package-agent-mail-plugin.sh" 2>&1)"; then
  printf '%s\n' "$package_output" >&2
  echo "Agent Mail plugin packaging failed; fix the package error above before running smoke." >&2
  exit 1
fi

"$wrapper" --help >/dev/null

if ! grep -aFq "$expected_main_reminder" "$packaged_binary"; then
  echo "packaged binary does not contain expected main-agent hook reminder text" >&2
  exit 1
fi
if ! grep -aFq "$expected_subagent_reminder" "$packaged_binary"; then
  echo "packaged binary does not contain expected subagent hook reminder text" >&2
  exit 1
fi
if ! grep -aFq "$expected_mcp_instructions" "$packaged_binary"; then
  echo "packaged binary does not contain expected stateless MCP instructions" >&2
  exit 1
fi

help_output="$("$wrapper" --help 2>&1)"
case "$help_output" in
  *"Commands:"*"hook"* ) ;;
  *)
    echo "help output does not advertise hook command" >&2
    printf '%s\n' "$help_output" >&2
    exit 1
    ;;
esac
case "$help_output" in
  *"Commands:"*"serve-mcp"* ) ;;
  *)
    echo "help output does not advertise serve-mcp command required by plugins/agent-mail/.mcp.json" >&2
    printf '%s\n' "$help_output" >&2
    exit 1
    ;;
esac

for retired in team read write coworkers contacts subagents ls; do
  if "$wrapper" "$retired" >/dev/null 2>&1; then
    echo "retired command still succeeds: $retired" >&2
    exit 1
  fi
done

python3 - "$marketplace" "$plugin" <<'PY'
import json
import sys
from pathlib import Path

marketplace = Path(sys.argv[1])
plugin = Path(sys.argv[2])

for path in [
    marketplace,
    plugin / ".codex-plugin" / "plugin.json",
    plugin / ".mcp.json",
    plugin / "hooks" / "hooks.json",
    plugin / "bin" / "agent-mail",
]:
    if not path.exists():
        raise SystemExit(f"missing {path}")

with marketplace.open() as handle:
    payload = json.load(handle)

plugins = payload.get("plugins", [])
entry = next((item for item in plugins if item.get("name") == "agent-mail"), None)
if entry is None:
    raise SystemExit("marketplace missing agent-mail")

source = entry.get("source", {})
if source.get("source") != "local" or source.get("path") != "./plugins/agent-mail":
    raise SystemExit("marketplace agent-mail source is wrong")

with (plugin / ".codex-plugin" / "plugin.json").open() as handle:
    manifest = json.load(handle)

if "hooks" in manifest:
    raise SystemExit("plugin manifest must not declare unsupported field hooks; use hooks/hooks.json")
if manifest.get("mcpServers") != "./.mcp.json":
    raise SystemExit("plugin manifest must point mcpServers at ./.mcp.json")

with (plugin / ".mcp.json").open() as handle:
    mcp = json.load(handle)

server = mcp.get("mcpServers", {}).get("agent_mail")
if not isinstance(server, dict):
    raise SystemExit(".mcp.json must define mcpServers.agent_mail")
if server.get("command") != "./bin/agent-mail":
    raise SystemExit(".mcp.json agent_mail command must be ./bin/agent-mail")
args = server.get("args")
if not isinstance(args, list) or args[:1] != ["serve-mcp"]:
    raise SystemExit(".mcp.json agent_mail args must start with serve-mcp")
if server.get("cwd") != ".":
    raise SystemExit(".mcp.json agent_mail cwd must be .")

interface = manifest.get("interface", {})
manifest_text = json.dumps(interface)
for retired in [
    "agent-mail team",
    "coworkers",
    "contacts",
    "agent-mail subagents",
    "agent_mail.team",
    "directory/debug",
    "native hosted-tool",
    "native agent_mail tools",
    "Codex core",
    "hook reminder only",
]:
    if retired in manifest_text:
        raise SystemExit(f"plugin manifest still mentions retired surface: {retired}")

capabilities = interface.get("capabilities", [])
if capabilities and not {"MCP", "Hooks"}.issubset(set(capabilities)):
    raise SystemExit("plugin manifest capabilities must include MCP and Hooks when present")
PY

if command -v codex >/dev/null 2>&1; then
  tmp_home="$(mktemp -d)"
  CODEX_HOME="$tmp_home" codex --enable plugins plugin marketplace add "$repo_root" >/dev/null
else
  echo "codex CLI not found; skipped isolated marketplace install smoke" >&2
fi

echo "Agent Mail plugin package smoke passed"
echo "Closed-loop GUI/subagent Agent Mail delivery proof is not faked by this script."
echo "Manual install: codex plugin marketplace add $repo_root"
