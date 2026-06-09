# Agent Messaging API Draft

Talking to other Codex agents is useful for coordination and coding tasks.
Prefer native app commands for visible UI; use app-server one-liners for
subagents or shell-only contexts.

## Native App Command

### agent.repo_threads
Use: find recent app threads. Works: agents with `codex_app`. Returns: `threads[]` with id, title, preview, status, cwd, createdAt, updatedAt.

```text
codex_app.list_threads({"limit":20})
```

### agent.message_thread
Use: send a visible app-level message to another thread/agent. Works: agents with `codex_app`. Put sender metadata first so the collapsed UI preview shows it. App also adds `codexDelegation.sourceThreadId`.

```text
codex_app.send_message_to_thread({"threadId":"<target_thread_id>","prompt":"FROM_THREAD_ID=<sender_thread_id> FROM_THREAD_NAME=\"<sender_thread_name>\" FROM_AGENT_NAME=<sender_agent_name>\n<message_body>"})
```

### agent.read_thread
Use: read another thread/agent by id. Works: agents with `codex_app`. Returns: thread metadata, turns, messages, tool summaries, and structured `codexDelegation`.

```text
codex_app.read_thread({"threadId":"<target_thread_id>","turnLimit":5,"includeOutputs":true,"maxOutputCharsPerItem":4000})
```

## App-Server One-Liners

### agent.repo_threads_for_subagents
Use: repo thread discovery from a subagent. Works: subagents and shell-only contexts. Returns: `thread_id`, `thread_name`, `status`, `updated_at`, `cwd`.

```bash
repo_cwd="$PWD"; { printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"repo-threads-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"thread/list","params":{"limit":20,"archived":false,"useStateDbOnly":false}}'; sleep 0.7; } | codex app-server --listen stdio:// 2>/dev/null | jq -r --arg cwd "$repo_cwd" 'select(.id==2) | .result.data[] | select(.cwd == $cwd) | "thread_id=\(.id) thread_name=\(.name // "<null>") status=\((.status | if type == "object" then .type else . end) // "<null>") updated_at=\(.updatedAt // "<null>") cwd=\(.cwd // "<null>")"'
```

### agent.read_thread_for_subagents
Use: read another thread by id from a subagent or shell-only context. Works: main agents and subagents. Returns: compact thread metadata plus recent user/agent messages and parsed source thread id.

```bash
target_thread_id="<target_thread_id>"; { printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"read-thread-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"thread/read\",\"params\":{\"threadId\":\"$target_thread_id\",\"includeTurns\":true}}"; sleep 0.7; } | codex app-server --listen stdio:// 2>/dev/null | jq -r 'select(.id==2) | .result.thread as $t | "thread_id=\($t.id) thread_name=\($t.name // "<null>") status=\(($t.status.type // $t.status) // "<null>")" , ($t.turns[-2:][] | "turn_id=\(.id) status=\(.status)" , (.items[] | if .type == "agentMessage" then "agent phase=\(.phase // "<null>") text=\(.text | gsub("\n"; " ") | .[0:260])" elif .type == "userMessage" then (.content[]? | select(.type == "text") | "user source_thread_id=\((.codexDelegation.sourceThreadId // (.text | capture("<source_thread_id>(?<id>[^<]+)</source_thread_id>").id?)) // "<none>") text=\(.text | gsub("\n"; " ") | .[0:260])") else empty end))'
```

### agent.identity
Use: get this agent's id/name/nickname and parent when present. Works: main agents and subagents. Returns: self plus parent metadata if subagent.

```bash
t="$({ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"identity-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"thread/read\",\"params\":{\"threadId\":\"$CODEX_THREAD_ID\",\"includeTurns\":false}}"; sleep 0.5; } | codex app-server --listen stdio:// 2>/dev/null | jq -c 'select(.id==2) | .result.thread')"; printf '%s\n' "$t" | jq -r '"thread_id=\(.id) thread_name=\(.name)" + (if .agentNickname then " nickname=\(.agentNickname)" else "" end) + (if .agentRole then " role=\(.agentRole)" else "" end)'; p="$(printf '%s\n' "$t" | jq -r '(.source | objects | .subAgent.thread_spawn.parent_thread_id) // empty')"; if [ -n "$p" ]; then { printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"identity-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"thread/read\",\"params\":{\"threadId\":\"$p\",\"includeTurns\":false}}"; sleep 0.5; } | codex app-server --listen stdio:// 2>/dev/null | jq -r 'select(.id==2) | .result.thread | "parent_thread_id=\(.id) parent_thread_name=\(.name)" + (if .agentNickname then " parent_agent_nickname=\(.agentNickname)" else "" end) + (if .agentRole then " parent_agent_role=\(.agentRole)" else "" end) + " source=\(.source)"'; fi
```

### agent.team
Use: get local coordination graph. Works: main agents and subagents. Returns: self, parent when present, direct children, and siblings when present.

```bash
s="$({ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"agent-team-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"thread/read\",\"params\":{\"threadId\":\"$CODEX_THREAD_ID\",\"includeTurns\":false}}"; sleep 0.5; } | codex app-server --listen stdio:// 2>/dev/null | jq -c 'select(.id==2) | .result.thread')"; l="$({ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"agent-team-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"thread/list","params":{"limit":500,"sourceKinds":["subAgent","subAgentThreadSpawn","subAgentReview","subAgentCompact","subAgentOther"],"archived":false,"useStateDbOnly":false}}'; sleep 0.7; } | codex app-server --listen stdio:// 2>/dev/null | jq -c 'select(.id==2) | .result.data')"; printf '%s\n' "$s" | jq -r '"self_thread_id=\(.id) self_thread_name=\(.name)" + (if .agentNickname then " self_nickname=\(.agentNickname)" else "" end) + (if .agentRole then " self_role=\(.agentRole)" else "" end)'; p="$(printf '%s\n' "$s" | jq -r '(.source | objects | .subAgent.thread_spawn.parent_thread_id) // empty')"; if [ -n "$p" ]; then { printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"agent-team-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"thread/read\",\"params\":{\"threadId\":\"$p\",\"includeTurns\":false}}"; sleep 0.5; } | codex app-server --listen stdio:// 2>/dev/null | jq -r 'select(.id==2) | .result.thread | "parent_thread_id=\(.id) parent_thread_name=\(.name)" + (if .agentNickname then " parent_nickname=\(.agentNickname)" else "" end) + (if .agentRole then " parent_role=\(.agentRole)" else "" end)'; fi; printf '%s\n' "$l" | jq -r --arg self "$(printf '%s\n' "$s" | jq -r '.id')" '.[] | select(((.source | objects | .subAgent.thread_spawn.parent_thread_id) // "") == $self) | "child_thread_id=\(.id) child_nickname=\(.agentNickname // .source.subAgent.thread_spawn.agent_nickname // "<null>") child_thread_name=\(.name // "<null>")" + (if .agentRole then " child_role=\(.agentRole)" else "" end)'; if [ -n "$p" ]; then printf '%s\n' "$l" | jq -r --arg self "$(printf '%s\n' "$s" | jq -r '.id')" --arg parent "$p" '.[] | select(((.source | objects | .subAgent.thread_spawn.parent_thread_id) // "") == $parent and .id != $self) | "sibling_thread_id=\(.id) sibling_nickname=\(.agentNickname // .source.subAgent.thread_spawn.agent_nickname // "<null>") sibling_thread_name=\(.name // "<null>")" + (if .agentRole then " sibling_role=\(.agentRole)" else "" end)'; fi
```

### agent.children
Use: list direct subagents this agent spawned. Works: main agents and subagents with children. Returns: `id`, `nickname`, optional `role`.

```bash
{ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"subagent-list-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' '{"jsonrpc":"2.0","id":2,"method":"thread/list","params":{"limit":200,"sourceKinds":["subAgent","subAgentThreadSpawn","subAgentReview","subAgentCompact","subAgentOther"],"archived":false,"useStateDbOnly":false}}'; sleep 0.5; } | codex app-server --listen stdio:// 2>/dev/null | jq -r --arg parent "$CODEX_THREAD_ID" 'select(.id==2) | .result.data[] | select((.source.subAgent.thread_spawn.parent_thread_id // "") == $parent) | "id=\(.id) nickname=\(.agentNickname // .source.subAgent.thread_spawn.agent_nickname // "")" + (if .agentRole then " role=\(.agentRole)" else "" end)'
```

### agent.siblings
Use: list subagents sharing this subagent's parent. Works: subagents only; main agents return no output. Returns: `id`, `nickname`, `name`.

```bash
{ printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"clientInfo":{"name":"identity-probe","title":null,"version":"0"},"capabilities":{"experimentalApi":true}}}'; sleep 0.2; printf '%s\n' "{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"thread/read\",\"params\":{\"threadId\":\"$CODEX_THREAD_ID\",\"includeTurns\":false}}"; sleep 0.2; printf '%s\n' '{"jsonrpc":"2.0","id":3,"method":"thread/list","params":{"limit":500,"sourceKinds":["subAgentThreadSpawn"],"archived":false}}'; sleep 0.7; } | codex app-server --listen stdio:// 2>/dev/null | jq -rs '(.[] | select(.id==2) | .result.thread) as $self | (($self.source | objects | .subAgent.thread_spawn.parent_thread_id) // empty) as $parent | select($parent != "") | .[] | select(.id==3) | .result.data[] | select(((.source | objects | .subAgent.thread_spawn.parent_thread_id) // "") == $parent and .id != $self.id) | "id=\(.id) nickname=\(.agentNickname // "<null>") name=\(.name // "<null>")"'
```
