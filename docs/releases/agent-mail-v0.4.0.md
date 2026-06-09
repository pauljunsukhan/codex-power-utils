# Agent Mail v0.4.0

Agent Mail v0.4.0 is a stateless Codex plugin release. It exposes MCP tools for
team discovery, non-turn thread-history mail, and real thread reads:

```text
agent_mail.my_team
agent_mail.repo_teams
agent_mail.write
agent_mail.read
```

Install from the Git marketplace tag:

```bash
codex plugin marketplace add pauljunsukhan/codex-power-utils --ref agent-mail-v0.4.0
codex plugin add agent-mail@codex-power-utils
```

Open a new Codex thread after installing so the MCP tools load. Then open Hooks
review or `/hooks` and trust the Agent Mail hook.

## Included Platforms

The plugin cartridge includes:

```text
plugins/agent-mail/bin/agent-mail-aarch64-apple-darwin
plugins/agent-mail/bin/agent-mail-x86_64-apple-darwin
```

SHA-256:

```text
b73e67287d97fb7865f37c4d4aa661700e3d355e7a9d2fb688c60eb5b9573b85  agent-mail-aarch64-apple-darwin
30aef701a73c9b15609e9faa31e0b96b724ec7df4f614df2d688f4d0fa34fabb  agent-mail-x86_64-apple-darwin
```

Other platforms can still build the Rust MCP server from source, but this
release only ships prebuilt macOS binaries.

## Validation

This release was validated with:

```bash
cargo test -p agent-mail
scripts/smoke-test-agent-mail-plugin.sh
python /Users/paulhan/.codex/skills/.system/plugin-creator/scripts/validate_plugin.py plugins/agent-mail
```
