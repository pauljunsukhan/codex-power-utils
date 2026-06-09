# Agent Mail MCP Plugin

Agent Mail is a stateless Codex plugin that exposes four MCP tools backed by
real Codex thread APIs.

```text
agent_mail.my_team
agent_mail.repo_teams
agent_mail.write
agent_mail.read
```

There is no Agent Mail database, mailbox, delivery queue, receipt table, reply
obligation table, or forward-window state. The plugin never claims delivery
from private state.

## Adapters

```text
thread/list          team and repo discovery
thread/read          identity, parent/child graph, and transcript reads
thread/resume        target materialization before injection when required
thread/inject_items  non-turn mail appended to target thread history
```

## Behavior

`my_team` reads the caller thread and groups real subagent threads by
`parent_thread_id`.

`repo_teams` lists real main threads in the current repo and attaches direct
subagents from real thread metadata.

`write` resolves a handle to a real target thread and appends a user message
item with sender metadata. If injection reports the thread is not found, it
calls `thread/resume` and retries injection in the same app-server session. It
does not start a turn and does not use a store.

`read` resolves a handle and returns `thread/read` history plus recent response
items from the target Codex session JSONL transcript when `thread/read` exposes
a transcript path.

## Role Hooks

Main-agent hook:

```text
You have Agent Mail for coordinating with Codex agents. Role: main agent. Use `agent_mail.my_team({"closed":true})` to list your real main/subagent team, `agent_mail.repo_teams({"closed":true})` to list real teams in this repo, `agent_mail.write({"to":"subagent:1","body":"...","requireReply":true})` to append non-terminating mail to another agent's real thread history, and `agent_mail.read({"target":"subagent:1","limit":10})` to read real visible thread context. Use handles from `my_team`/`repo_teams`. Agent Mail has no private store.
```

Subagent hook:

```text
You have Agent Mail context from your main agent. Role: subagent. Do not initiate Agent Mail with `agent_mail.write`; use `agent_mail.read({"target":"main","limit":10})` when you need the main thread's visible context. Reply by completing your normal subagent turn and continue assigned work unless the main agent explicitly changes it. Agent Mail has no private store.
```

## Write Contract

`agent_mail.write` accepts `to`, `body`, `interrupt`, `forwardNext`, and
`requireReply` for API compatibility. Because there is no store, `forwardNext`
and `requireReply` do not create persistent obligations.

The tool returns a delivery object with:

```text
state=delivered
deliveryPath=app_server
deliveryScope=thread_history
turnAddressed=false
triggeredTurn=false
store=null
```

If `thread/inject_items` still fails after materialization, the tool fails. It
must not return queued mail.

## Proof

Closed-loop proof is:

```text
agent_mail.my_team reads real main/subagent thread ids
agent_mail.repo_teams reads real repo threads
agent_mail.write appends to a real target thread
agent_mail.read sees that target thread history and transcript-file injected items
subagents reply through normal visible output, not agent_mail.write
```
