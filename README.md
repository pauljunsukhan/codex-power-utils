# Codex Power Utils

Agent Mail is native GUI mail for Codex-agent coordination.

The product direction is native GUI mail: Codex core owns live agent discovery,
sender identity, delivery, read/unread state, interrupt behavior, and GUI rendering
through hosted `agent_mail.*` tools. This repo currently ships the public
plugin/CLI shell for install, hooks, and future host bridge work while the
host-native runtime lands.

```bash
agent-mail team
```

`agent-mail team` shows the current main/subagent family with usable
addresses such as `main`, `subagent`, and `subagent:1`. It is the only public
directory command for now. Broad search and outside-family discovery are
intentionally out of the first Agent Mail surface.

`agent-mail write` is reserved for host-native GUI mail. `agent-mail read` is
reserved for host-native transcript browsing of reachable agents. Neither command
uses SQLite snapshots, stale history, local mailbox files, or terminal scraping.

## Install

```bash
codex plugin marketplace add pauljunsukhan/codex-power-utils
```

Then enable Agent Mail in Codex Plugins, open Hooks review or `/hooks`, and
trust the Agent Mail hooks. Hooks only teach the discovery/debug surface until
native `agent_mail.*` tools exist in the host.

## Development

Utilities in this repo are implemented in Rust. Build and test with Cargo:

```bash
cargo test
cargo run -p agent-mail -- team
cargo run -p agent-mail -- write subagent:1 "Can you check this?"
```

Package the current host binary into the Codex plugin cartridge:

```bash
scripts/package-agent-mail-plugin.sh
scripts/smoke-test-agent-mail-plugin.sh
```

Generated `plugins/agent-mail/bin/agent-mail-*` binaries are release artifacts.
The source cartridge keeps the portable `bin/agent-mail` wrapper.

## Repo Layout

- `crates/agent-mail`: Rust CLI for directory/debug and future host bridge.
- `plugins/agent-mail`: installable Codex plugin cartridge.
- `docs/agent-mail.md`: customer-facing guide, install flow, and current CLI.
- `docs/agent-mail-implementation-spec.md`: strict host-native implementation contract.
- `.agents/plugins/marketplace.json`: repo-local marketplace entry.
- `vendor/openai-codex`: dev-only protocol oracle, not a runtime dependency.

## More

- [docs/agent-mail.md](docs/agent-mail.md): product guide and install details.
- [docs/agent-mail-implementation-spec.md](docs/agent-mail-implementation-spec.md): Codex host implementation spec.
