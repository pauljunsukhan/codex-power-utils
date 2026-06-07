#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
wrapper="$repo_root/plugins/agent-mail/bin/agent-mail"
marketplace="$repo_root/.agents/plugins/marketplace.json"
plugin="$repo_root/plugins/agent-mail"

"$repo_root/scripts/package-agent-mail-plugin.sh" >/dev/null
cargo run -p agent-mail -- --help >/dev/null
"$wrapper" --help >/dev/null

for hook in session-start subagent-start user-prompt-submit; do
  output="$(printf '{}' | "$wrapper" hook "$hook")"
  case "$output" in
    *"Agent Mail"*"agent-mail"*) ;;
    *)
      echo "hook smoke failed for $hook: $output" >&2
      exit 1
      ;;
  esac
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
PY

if command -v codex >/dev/null 2>&1; then
  tmp_home="$(mktemp -d)"
  trap 'rm -rf "$tmp_home"' EXIT
  CODEX_HOME="$tmp_home" codex --enable plugins plugin marketplace add "$repo_root" >/dev/null
  CODEX_HOME="$tmp_home" codex --enable plugins plugin list | grep -q "agent-mail@codex-power-utils"
  CODEX_HOME="$tmp_home" codex --enable plugins plugin add agent-mail@codex-power-utils >/dev/null
  CODEX_HOME="$tmp_home" codex --enable plugins plugin list | grep -q "installed, enabled"
else
  echo "codex CLI not found; skipped isolated marketplace install smoke" >&2
fi

echo "Agent Mail plugin smoke passed"
echo "Manual install: codex plugin marketplace add $repo_root"
