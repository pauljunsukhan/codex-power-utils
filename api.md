# Agent Mail API

Agent Mail is a stateless MCP front door over real Codex thread APIs.

There is no Agent Mail store, plugin mailbox, receipt ledger, forward window
table, reply obligation table, or stale team cache. If Agent Mail cannot prove
something from Codex thread state, it must say so.

## Public Tools

```text
agent_mail.my_team
agent_mail.repo_teams
agent_mail.write
agent_mail.read
```

No aliases are part of the public API.

## Backing Adapters

Agent Mail uses the same surfaces proven in `skills/agent-messaging/draft2.md`,
plus one non-turn app-server write adapter.

```text
thread/list          discover real repo threads and subagent threads
thread/read          read real thread metadata, turns, messages, and delegation context
thread/resume        materialize a target thread when app-server requires it before injection
thread/inject_items  append non-terminating mail to real target thread history
```

Native app commands remain the preferred manual/operator form when directly
available:

```text
codex_app.list_threads
codex_app.send_message_to_thread
codex_app.read_thread
```

The MCP implementation must not create a second mailbox to simulate them.

## Roles

Main agents may use all four tools.

Subagents may use `my_team`, `repo_teams`, and `read`, but ordinary subagent
coordination must not use `write`. A subagent replies by completing its normal
subagent turn and continuing its assigned work unless the main agent explicitly
changes that work.

`agent_mail.write` from a subagent fails without mutating anything.

## Tool Schemas

```ts
agent_mail.my_team(args?: {
  closed?: boolean;
}): AgentMailTeam
```

```ts
agent_mail.repo_teams(args?: {
  closed?: boolean;
}): AgentMailRepoTeams
```

```ts
agent_mail.write(args: {
  to: string;
  body: string;
  interrupt?: boolean;
  forwardNext?: number;
  requireReply?: boolean;
}): AgentMailDelivery
```

```ts
agent_mail.read(args: {
  target: string;
  limit?: number;
  since?: string;
  until?: string;
}): AgentThreadRead
```

`forwardNext` and `requireReply` are accepted for compatibility but are not
stateful. With no store, they can only be included in the delivered message
body or returned as unsupported metadata.

## Handles

Handles are derived live from thread data on each call.

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

`subagent` resolves only when exactly one matching subagent exists. Ambiguity
fails with candidate handles. Agent Mail must not guess.

`repo-team:N` ordering comes from current `thread/list` results, usually newest
updated main thread first. It is live discovery, not persistent identity.

## Identity

Every listed agent includes real thread-derived identity:

```ts
interface AgentMailIdentity {
  handle: string;
  id: string;
  kind: "main" | "subagent";
  name?: string;
  role?: string;
  state?: string;
  isCaller?: boolean;
  canWrite: boolean;
  canRead: boolean;
  threadId: string;
  parentThreadId?: string;
  teamId: string;
  repoId?: string;
  workspace?: string;
  cwd?: string;
  source: "app_server";
  freshness: "live";
  cached: false;
  stale: false;
}
```

## Write

`agent_mail.write` is non-turn, non-store mail delivery.

It resolves the target handle to a real thread id and appends a user message
item to that thread history with `thread/inject_items`. If app-server reports
the target is not found for injection, Agent Mail opens one app-server session,
calls `thread/resume` with `serviceTier: "flex"`, and retries
`thread/inject_items` in that same session.

The injected message body starts with sender metadata:

```text
FROM_THREAD_ID=<caller_thread_id> FROM_THREAD_NAME="<caller_thread_name>" FROM_AGENT_NAME=<caller_agent_name>
<body>
```

This mirrors the practical lesson from `draft2.md`: sender metadata should be
visible early, while the thread id remains the durable reply route.

Write is not turn-addressed and does not start a target turn. `thread/resume`
is only an in-process materialization step for app-server; it is not mail
delivery and must not be represented as a target reply obligation. Write updates
the target's real thread history so the target can read the context and continue
work. It must not create one-shot "reply and stop" behavior.

```ts
interface AgentMailDelivery {
  mailId: string;
  state: "delivered" | "failed";
  from: AgentMailIdentity;
  to: AgentMailIdentity;
  deliveryPath: "app_server";
  deliveryScope: "thread_history";
  turnAddressed: false;
  triggeredTurn: false;
  visibleDeliveryConfirmed: boolean;
  proof: {
    kind: "thread_inject_items" | "target_readback";
    targetThreadId: string;
    messageId: string;
    resumedBeforeInject: boolean;
  };
  store: null;
}
```

## Read

`agent_mail.read` resolves the target handle and calls `thread/read` with
`includeTurns: true`. When `thread/read` exposes a target transcript `path`,
Agent Mail also reads that real Codex session JSONL file and returns recent
response items from it. This is necessary because non-turn injected items are
persisted in the session transcript but may not appear in `thread/read.turns`.

The result includes compact recent items, `turnItems`, `sessionFileItems`, and
the raw thread payload.

It never reads Agent Mail-only state because there is none.

## Hooks

Hooks only provide role-aware reminder text. They do not register agents, queue
mail, mark receipts, or persist context.

Main-agent hook:

```text
You have Agent Mail for coordinating with Codex agents. Role: main agent. Use `agent_mail.my_team({"closed":true})` to list your real main/subagent team, `agent_mail.repo_teams({"closed":true})` to list real teams in this repo, `agent_mail.write({"to":"subagent:1","body":"...","requireReply":true})` to append non-terminating mail to another agent's real thread history, and `agent_mail.read({"target":"subagent:1","limit":10})` to read real visible thread context. Use handles from `my_team`/`repo_teams`. Agent Mail has no private store.
```

Subagent hook:

```text
You have Agent Mail context from your main agent. Role: subagent. Do not initiate Agent Mail with `agent_mail.write`; use `agent_mail.read({"target":"main","limit":10})` when you need the main thread's visible context. Reply by completing your normal subagent turn and continue assigned work unless the main agent explicitly changes it. Agent Mail has no private store.
```

## Strict Limits

Agent Mail must not:

```text
- Persist a private Agent Mail mailbox.
- Mark store-only messages as delivered.
- Maintain forward windows or reply obligations.
- Use stale hook state as live team discovery.
- Ask subagents to reply with agent_mail.write.
- Start a target turn for ordinary mail delivery.
- Hide adapter failures behind queued state.
```

## Acceptance Criteria

```text
- my_team is computed from thread/read plus thread/list.
- repo_teams is computed from live thread/list results.
- write appends to the real target thread with thread/inject_items, using thread/resume only when required to materialize the target.
- read returns real thread/read history plus real session transcript items when available.
- subagent write fails.
- grep finds no Store, plugin_store, hook_store, queued mailbox, or receipt ledger in the implementation.
- cargo test passes.
```
