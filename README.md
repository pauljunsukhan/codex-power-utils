# Codex Power Utils

Agent Mail is stateless coordination for Codex-agent teams.

This repo ships the installable Agent Mail plugin cartridge and its Rust MCP
server. The MCP tools are backed by real Codex thread APIs and do not use a
private Agent Mail store.

Agents use the MCP tools directly:

```text
agent_mail.my_team
agent_mail.repo_teams
agent_mail.write
agent_mail.read
```

The plugin binary implements `agent-mail serve-mcp`, `agent-mail hook <event>`,
and `agent-mail doctor`.

## Install

```bash
codex plugin marketplace add pauljunsukhan/codex-power-utils
```

Then enable Agent Mail in Codex Plugins, open Hooks review or `/hooks`, and
trust the Agent Mail hook.

## Development

Utilities in this repo are implemented in Rust. Build and test with Cargo:

```bash
cargo test
cargo run -p agent-mail -- doctor
```

Package the current host binary into the Codex plugin cartridge:

```bash
scripts/package-agent-mail-plugin.sh
scripts/smoke-test-agent-mail-plugin.sh
```

Generated `plugins/agent-mail/bin/agent-mail-*` binaries are ignored local
packaging output. The source cartridge keeps the portable `bin/agent-mail`
wrapper.

## Repo Layout

- `api.md`: authoritative Agent Mail MCP API.
- `crates/agent-mail`: Rust MCP and hook binary for the installable plugin cartridge.
- `plugins/agent-mail`: installable Codex plugin cartridge.
- `docs/agent-mail-mcp-plugin.md`: plugin behavior guide.
- `docs/agent-mail-mcp-plugin-implementation-spec.md`: stateless implementation contract.
- `skills/agent-messaging/draft2.md`: manually verified app/thread command reference.
- `.agents/plugins/marketplace.json`: repo-local marketplace entry.

## More

- [api.md](api.md): authoritative public API.
- [docs/agent-mail-mcp-plugin.md](docs/agent-mail-mcp-plugin.md): plugin guide.
- [docs/agent-mail-mcp-plugin-implementation-spec.md](docs/agent-mail-mcp-plugin-implementation-spec.md): implementation spec.
