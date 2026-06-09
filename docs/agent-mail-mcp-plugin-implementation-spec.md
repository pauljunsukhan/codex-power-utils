# Agent Mail Stateless Implementation Spec

This plugin is a thin MCP adapter over Codex app-server thread APIs. It does
not own durable coordination state.

## Public Tools

```text
my_team
repo_teams
write
read
```

## No Store

Do not implement or reintroduce:

```text
Store
store.db
agent-mail-store.json
plugin mailbox
queued mail
receipts table
reply_obligations table
forward_windows table
hook registration table
stale/cached team records
```

Hooks emit reminder text only.

## App-Server Client

Each tool may start `codex app-server -c service_tier="flex" --listen stdio://`
and send newline JSON RPC messages:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"agent-mail-adapter","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}
```

Use these methods:

```text
thread/list
thread/read
thread/resume
thread/inject_items
```

`thread/list` is used twice when needed: once for normal/main threads and once
with subagent `sourceKinds`.

## Caller Binding

Bind the caller from MCP metadata or environment:

```text
CODEX_THREAD_ID
CODEX_SESSION_ID
CODEX_CONVERSATION_ID
AGENT_MAIL_SESSION_ID
params._meta.threadId
params._meta.thread_id
```

If no caller thread can be bound for a caller-sensitive tool, fail.

## Tool Behavior

`my_team`:

```text
1. thread/read caller.
2. If caller has parent_thread_id, thread/read parent.
3. thread/list subagent sourceKinds.
4. Return main plus subagents where parent_thread_id matches main.
```

`repo_teams`:

```text
1. Determine cwd from caller thread or process cwd.
2. thread/list normal threads for cwd.
3. thread/list subagent sourceKinds for cwd.
4. Group subagents under parent_thread_id.
5. Number handles repo-team:N by current list order.
```

`write`:

```text
1. Reject subagent callers.
2. Resolve target handle from live team/repo discovery.
3. Prefix body with FROM_THREAD_ID, FROM_THREAD_NAME, FROM_AGENT_NAME.
4. thread/inject_items a raw user message into the target thread.
5. If injection reports `thread not found`, open one app-server session, thread/resume the target with serviceTier flex, and retry injection in that same session.
6. Optionally thread/read target to confirm readback.
7. Return delivered or fail.
```

The injected raw item:

```json
{
  "type": "message",
  "id": "agent_mail_<stable_id>",
  "role": "user",
  "content": [
    {
      "type": "input_text",
      "text": "<metadata-prefixed body>"
    }
  ]
}
```

`read`:

```text
1. Resolve target handle from live team/repo discovery.
2. thread/read target with includeTurns=true.
3. If thread/read exposes a transcript path, read recent response_item payloads from that real Codex JSONL transcript.
4. Return compact recent items, turnItems, sessionFileItems, and raw thread payload.
```

## Handles

Supported handles:

```text
main
subagent
subagent:N
Name
role:Role
Name:Role
threadId
repo-team:N/main
repo-team:N/subagent:M
repo-team:N/Name
repo-team:N/role:Role
```

Ambiguity fails. No guessing.

## Tests

Unit tests must cover:

```text
subagent parent parsing
handle resolution
sender metadata prefix
role-aware hook copy
JSON-RPC line parsing
```

Runtime proof must use real Codex app-server calls.
