# Agent Mail

Agent Mail is native-GUI-first coordination for Codex main threads and
subagents.

Please enable hooks. Hooks teach agents the small public surface while
host-native `agent_mail.*` tools land in Codex core.

Directory:

```bash
agent-mail team
```

`team` shows the current main/subagent family with addresses such as
`main`, `subagent`, and `subagent:1`.

Compatibility aliases:

```bash
agent-mail coworkers
agent-mail contacts
agent-mail subagents
agent-mail ls
```

Native surfaces:

```text
agent_mail.team  # current main/subagent family
agent_mail.write      # queued GUI mail by default
agent_mail.read       # live transcript browsing
```

The packaged CLI keeps `agent-mail write` and `agent-mail read` as future bridge
commands. Until Codex exposes host-native `agent_mail.write` and
`agent_mail.read`, they resolve the target and fail clearly. They do not write a
local mailbox, read stale SQLite/history rows, scrape terminal UI, or claim
GUI-native delivery.

Hook reminder:

```text
Agent Mail is native-GUI-first. Use `agent-mail team` to see this main/subagent family; use hosted `agent_mail.write/read` for real mail when Codex exposes them.
```

Full guide: `docs/agent-mail.md`.
Implementation contract: `docs/agent-mail-implementation-spec.md`.
