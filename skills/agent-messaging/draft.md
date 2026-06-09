# Agent Messaging Draft

Talking to agents in other Codex threads is useful for agent coordination and
coding tasks: one thread can ask another to inspect context, run an independent
check, or report back without turning the current thread into a giant transcript.

## Thread Messaging

Return your own thread id, thread name, and subagent nickname when present:

```bash
{ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"identity-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"thread/read\",\"params\":{\"threadId\":\"$CODEX_THREAD_ID\",\"includeTurns\":false}}"; sleep 0.5; } | codex app-server --listen stdio:// 2>/dev/null | jq -r 'select(.id==2) | .result.thread | "thread_id=\(.id) thread_name=\(.name)" + (if .agentNickname then " nickname=\(.agentNickname)" else "" end)'
```

Return your direct subagents with id and nickname:

```bash
{ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"subagent-list-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"thread/list","params":{"limit":200,"sourceKinds":["subAgent","subAgentThreadSpawn","subAgentReview","subAgentCompact","subAgentOther"],"archived":false,"useStateDbOnly":false}}'; sleep 0.5; } | codex app-server --listen stdio:// 2>/dev/null | jq -r --arg parent "$CODEX_THREAD_ID" 'select(.id==2) | .result.data[] | select((.source.subAgent.thread_spawn.parent_thread_id // "") == $parent) | "id=\(.id) nickname=\(.agentNickname // .source.subAgent.thread_spawn.agent_nickname // "")" + (if .agentRole then " role=\(.agentRole)" else "" end)'
```

When running as a subagent, return sibling subagents that share your parent:

```bash
{ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"identity-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"thread/read\",\"params\":{\"threadId\":\"$CODEX_THREAD_ID\",\"includeTurns\":false}}"; sleep 0.2; printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"thread/list","params":{"limit":500,"sourceKinds":["subAgentThreadSpawn"],"archived":false}}'; sleep 0.7; } | codex app-server --listen stdio:// 2>/dev/null | jq -rs '(.[] | select(.id==2) | .result.thread) as $self | (($self.source | objects | .subAgent.thread_spawn.parent_thread_id) // empty) as $parent | select($parent != "") | .[] | select(.id==3) | .result.data[] | select(((.source | objects | .subAgent.thread_spawn.parent_thread_id) // "") == $parent and .id != $self.id) | "id=\(.id) nickname=\(.agentNickname // "<null>") name=\(.name // "<null>")"'
```

Create a new project thread:

```text
codex_app.create_thread
```

Send a GUI-visible follow-up:

```text
codex_app.send_message_to_thread
```

Read recent turns from another thread:

```text
codex_app.read_thread
```

Find candidate threads:

```text
codex_app.list_threads
```

Basic loop:

```text
create_thread -> send_message_to_thread -> read_thread
```

The receiver sees a `codex_delegation` wrapper with `source_thread_id`, so it
can reply without the sender thread ID being written in the body.

## Subagents

Create your own subagent:

```text
multi_agent_v1.spawn_agent
```

Manage it:

```text
multi_agent_v1.send_input
multi_agent_v1.wait_agent
multi_agent_v1.resume_agent
multi_agent_v1.close_agent
```

Save the returned `agent_id` and `nickname` immediately.

To create a subagent under another thread, message that parent thread:

```text
send_message_to_thread(parentThreadId, "Please spawn a subagent for...")
read_thread(parentThreadId)
```

If the subagent needs the app-level `source_thread_id`, have the parent include
it explicitly in the subagent prompt.

## Agent Mail

Use these for main/subagent mail-style coordination:

```text
agent_mail.my_team
agent_mail.repo_teams
agent_mail.write
agent_mail.read
```

Useful loop:

```text
my_team -> write -> wait_agent/read -> read
```

## CLI And App Server Probes

Start Codex in a repo:

```bash
codex -C /path/to/repo
codex --enable plugins -C /path/to/repo
```

Send a one-off debug message:

```bash
codex debug app-server send-message-v2 "message text"
```

Generate app-server protocol types:

```bash
codex app-server generate-ts --experimental --out /tmp/codex-app-protocol
```

Talk to app-server over stdio:

```bash
codex app-server --listen stdio://
```

Then send JSON-RPC lines:

```json
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}
```

```json
{"jsonrpc":"2.0","id":2,"method":"thread/list","params":{"limit":10,"sourceKinds":[],"archived":false}}
```

```json
{"jsonrpc":"2.0","id":3,"method":"thread/turns/list","params":{"threadId":"<thread-id>","limit":20,"sortDirection":"desc","itemsView":"full"}}
```

Use a control socket if one exists:

```bash
codex app-server proxy --sock /path/to/app-server-control.sock
```

## Useful Commands

These exist or looked useful, but are not yet the core skill path:

```text
codex_app.fork_thread
codex_app.handoff_thread
codex_app.set_thread_title
codex_app.set_thread_pinned
codex_app.set_thread_archived
```

```text
thread/read
thread/loaded/list
thread/turns/items/list
turn/start
turn/interrupt
thread/inject_items
```

Raw session recovery:

```bash
rg "multi_agent_v1|spawn_agent|wait_agent|close_agent" ~/.codex/sessions
jq ... ~/.codex/sessions/...jsonl
```
