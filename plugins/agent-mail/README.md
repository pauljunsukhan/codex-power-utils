# Agent Mail

Agent Mail is stateless coordination for Codex agents.

It exposes four MCP tools:

```text
agent_mail.my_team
agent_mail.repo_teams
agent_mail.write
agent_mail.read
```

The tools are backed by real Codex thread APIs:

```text
thread/list
thread/read
thread/resume
thread/inject_items
```

There is no Agent Mail store or plugin mailbox.

## Hooks

Hooks only add role-aware reminder text.

Main agents can list teams, write non-terminating mail, and read thread
context. Subagents should read the main thread and reply through normal visible
output.

## Proof

Good proof is live thread evidence:

```text
my_team shows real main/subagent ids
repo_teams shows real repo threads
write appends to a real target thread, resuming in the same app-server session only when injection needs materialization
read shows real target thread history
read also surfaces transcript-file items that thread/read omits
subagents do not use write as a reply path
```

Smoke tests are useful, but store-level tests are not proof because no store
exists.
