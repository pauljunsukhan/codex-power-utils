use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;
use serde_json::{Map, Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::fs;
use std::io::{self, BufRead, Read, Write};
use std::path::Path;
use std::process::{Command as ProcessCommand, Stdio};
use std::thread::sleep;
use std::time::Duration;

const AGENT_MAIL_MAIN_REMINDER: &str = "You have Agent Mail for coordinating with Codex agents. Role: main agent. Use `agent_mail.my_team({\"closed\":true})` to list your real main/subagent team, `agent_mail.repo_teams({\"closed\":true})` to list real teams in this repo, `agent_mail.write({\"to\":\"subagent:1\",\"body\":\"...\",\"requireReply\":true})` to append non-terminating mail to another agent's real thread history, and `agent_mail.read({\"target\":\"subagent:1\",\"limit\":10})` to read real visible thread context. Use handles from `my_team`/`repo_teams`. Agent Mail has no private store.";
const AGENT_MAIL_SUBAGENT_REMINDER: &str = "You have Agent Mail context from your main agent. Role: subagent. Do not initiate Agent Mail with `agent_mail.write`; use `agent_mail.read({\"target\":\"main\",\"limit\":10})` when you need the main thread's visible context. Reply by completing your normal subagent turn and continue assigned work unless the main agent explicitly changes it. Agent Mail has no private store.";
const AGENT_MAIL_SERVER_INSTRUCTIONS: &str = "Agent Mail is a stateless adapter over Codex thread APIs. `my_team`, `repo_teams`, and `read` use real thread/list and thread/read data. `write` appends non-terminating mail to the target thread history with thread/inject_items, resuming the target thread first when app-server requires materialization. No Agent Mail store or plugin mailbox exists.";

#[derive(Debug, Parser)]
#[command(name = "agent-mail")]
#[command(about = "Stateless Agent Mail MCP server backed by Codex thread APIs")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Run the Agent Mail MCP stdio server.
    ServeMcp,
    /// Emit model-visible Agent Mail context for a Codex hook event.
    Hook { event: HookEvent },
    /// Print stateless adapter diagnostics.
    Doctor,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum HookEvent {
    SessionStart,
    SubagentStart,
    UserPromptSubmit,
    PostToolUse,
    Stop,
    SubagentStop,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ThreadInfo {
    id: String,
    name: Option<String>,
    status: Option<String>,
    cwd: Option<String>,
    created_at: Option<String>,
    updated_at: Option<String>,
    agent_nickname: Option<String>,
    agent_role: Option<String>,
    parent_thread_id: Option<String>,
    source: Value,
    raw: Value,
}

impl ThreadInfo {
    fn is_subagent(&self) -> bool {
        self.parent_thread_id.is_some() || source_mentions_subagent(&self.source)
    }

    fn display_name(&self) -> String {
        self.agent_nickname
            .clone()
            .or_else(|| self.name.clone())
            .unwrap_or_else(|| self.id.clone())
    }
}

#[derive(Debug, Clone)]
struct Team {
    main: ThreadInfo,
    subagents: Vec<ThreadInfo>,
}

#[derive(Debug, Clone)]
struct RepoTeam {
    index: usize,
    main: ThreadInfo,
    subagents: Vec<ThreadInfo>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    match Cli::parse().command {
        Command::ServeMcp => serve_mcp(),
        Command::Hook { event } => run_hook(event),
        Command::Doctor => run_doctor(),
    }
}

fn run_doctor() -> Result<(), String> {
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "mode": "stateless",
            "store": null,
            "adapters": ["thread/list", "thread/read", "thread/resume", "thread/inject_items"],
            "codexThreadId": env_session_id()
        }))
        .map_err(|err| err.to_string())?
    );
    Ok(())
}

fn run_hook(event: HookEvent) -> Result<(), String> {
    if matches!(event, HookEvent::Stop | HookEvent::SubagentStop) {
        println!("{}", "{}");
        return Ok(());
    }
    let input = read_stdin_json().unwrap_or(Value::Object(Map::new()));
    let reminder = if hook_input_is_subagent(event, &input) {
        AGENT_MAIL_SUBAGENT_REMINDER
    } else {
        AGENT_MAIL_MAIN_REMINDER
    };
    let payload = json!({
        "hookSpecificOutput": {
            "hookEventName": hook_event_name(event),
            "additionalContext": format!("<agent_mail>\n{}\n</agent_mail>", reminder)
        }
    });
    println!(
        "{}",
        serde_json::to_string(&payload).map_err(|err| err.to_string())?
    );
    Ok(())
}

fn serve_mcp() -> Result<(), String> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let mut bound_caller_thread_id: Option<String> = None;

    loop {
        let Some(message) = read_json_rpc_message(&mut input)? else {
            break;
        };
        if let Some(response) = handle_json_rpc_message(message, &mut bound_caller_thread_id) {
            println!(
                "{}",
                serde_json::to_string(&response).map_err(|err| err.to_string())?
            );
            io::stdout().flush().map_err(|err| err.to_string())?;
        }
    }
    Ok(())
}

fn handle_json_rpc_message(
    message: Value,
    bound_caller_thread_id: &mut Option<String>,
) -> Option<Value> {
    let id = message.get("id").cloned();
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let Some(id) = id else {
        return None;
    };

    let result = match method {
        "initialize" => Ok(json!({
            "protocolVersion": params
                .get("protocolVersion")
                .and_then(Value::as_str)
                .unwrap_or("2025-03-26"),
            "capabilities": { "tools": {} },
            "serverInfo": {
                "name": "agent-mail",
                "version": env!("CARGO_PKG_VERSION")
            },
            "instructions": AGENT_MAIL_SERVER_INSTRUCTIONS
        })),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({ "tools": tool_descriptors() })),
        "tools/call" => handle_tool_call(params, bound_caller_thread_id).map(mcp_tool_result),
        _ => Err(format!("method not found: {method}")),
    };

    Some(match result {
        Ok(result) => json!({ "jsonrpc": "2.0", "id": id, "result": result }),
        Err(message) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": {
                "code": -32603,
                "message": message
            }
        }),
    })
}

fn handle_tool_call(
    params: Value,
    bound_caller_thread_id: &mut Option<String>,
) -> Result<Value, String> {
    let name = required_string(&params, "name")?;
    let args = params
        .get("arguments")
        .cloned()
        .unwrap_or(Value::Object(Map::new()));
    let caller_thread_id = caller_thread_id(&params)
        .or_else(|| bound_caller_thread_id.clone())
        .or_else(env_session_id);
    if let Some(thread_id) = caller_thread_id.as_ref() {
        *bound_caller_thread_id = Some(thread_id.clone());
    }

    match name.as_str() {
        "my_team" => tool_my_team(caller_thread_id.as_deref(), &args),
        "repo_teams" => tool_repo_teams(caller_thread_id.as_deref(), &args),
        "write" => tool_write(caller_thread_id.as_deref(), &args),
        "read" => tool_read(caller_thread_id.as_deref(), &args),
        other => Err(format!("unknown Agent Mail tool: {other}")),
    }
}

fn mcp_tool_result(value: Value) -> Value {
    json!({
        "content": [
            {
                "type": "text",
                "text": serde_json::to_string_pretty(&value).unwrap_or_else(|_| value.to_string())
            }
        ],
        "structuredContent": value
    })
}

fn tool_my_team(caller_thread_id: Option<&str>, args: &Value) -> Result<Value, String> {
    reject_unknown(args, &["closed"])?;
    let caller = read_caller(caller_thread_id)?;
    let team = team_for(&caller)?;
    Ok(team_view(&team, &caller))
}

fn tool_repo_teams(caller_thread_id: Option<&str>, args: &Value) -> Result<Value, String> {
    reject_unknown(args, &["closed"])?;
    let caller = caller_thread_id.and_then(|id| read_thread(id, false).ok());
    let cwd = caller
        .as_ref()
        .and_then(|thread| thread.cwd.clone())
        .or_else(current_cwd);
    let repo_teams = repo_teams(cwd.as_deref())?;
    let own_root = caller
        .as_ref()
        .and_then(|thread| root_thread_id(thread).ok());
    let own_team_handle = own_root.as_ref().and_then(|root| {
        repo_teams
            .iter()
            .find(|team| &team.main.id == root)
            .map(|team| format!("repo-team:{}", team.index))
    });
    Ok(json!({
        "caller": caller.as_ref().map(|thread| identity_view(thread, "caller", Some(thread))),
        "ownTeamHandle": own_team_handle,
        "repoId": cwd,
        "workspace": cwd.as_deref().and_then(workspace_name),
        "teams": repo_teams.iter().map(repo_team_view).collect::<Vec<_>>(),
        "source": "app_server",
        "store": null
    }))
}

fn tool_write(caller_thread_id: Option<&str>, args: &Value) -> Result<Value, String> {
    reject_unknown(
        args,
        &["to", "body", "interrupt", "forwardNext", "requireReply"],
    )?;
    let caller = read_caller(caller_thread_id)?;
    if caller.is_subagent() {
        return Err(
            "subagents may read Agent Mail context but must not initiate agent_mail.write"
                .to_string(),
        );
    }
    let to = required_string(args, "to")?;
    let body = required_string(args, "body")?;
    if body.trim().is_empty() {
        return Err("body cannot be empty".to_string());
    }
    let target = resolve_target(&caller, &to)?;
    let prompt = metadata_prefixed_body(&caller, body.trim());
    let mail_id = format!(
        "agent_mail_{}",
        stable_id(&format!("{}:{}:{}", caller.id, target.id, prompt))
    );

    let resumed_before_inject = inject_user_message(&target.id, &mail_id, &prompt)?;
    let readback_confirmed = confirm_thread_contains_text(&target, &prompt);

    Ok(json!({
        "mailId": mail_id,
        "state": "delivered",
        "from": identity_view(&caller, "main", Some(&caller)),
        "to": identity_view(&target, &target_handle_in_context(&caller, &target), Some(&caller)),
        "deliveryPath": "app_server",
        "deliveryScope": "thread_history",
        "turnAddressed": false,
        "triggeredTurn": false,
        "visibleDeliveryConfirmed": readback_confirmed,
        "proof": {
            "kind": if readback_confirmed { "target_readback" } else { "thread_inject_items" },
            "targetThreadId": target.id,
            "messageId": mail_id,
            "resumedBeforeInject": resumed_before_inject
        },
        "store": null,
        "unsupportedWithoutStore": {
            "forwardNext": args.get("forwardNext").cloned().unwrap_or(Value::Null),
            "requireReply": args.get("requireReply").cloned().unwrap_or(Value::Null)
        }
    }))
}

fn tool_read(caller_thread_id: Option<&str>, args: &Value) -> Result<Value, String> {
    reject_unknown(args, &["target", "limit", "since", "until"])?;
    let caller = read_caller(caller_thread_id)?;
    let target_handle = required_string(args, "target")?;
    let limit = optional_u32(args, "limit").unwrap_or(10).clamp(1, 100);
    let target = resolve_target(&caller, &target_handle)?;
    let thread = read_thread(&target.id, true)?;
    let turn_items = compact_thread_items(&thread.raw, limit as usize);
    let session_file_items_result = compact_session_file_items(&thread.raw, limit as usize);
    let session_file_error = session_file_items_result.as_ref().err().cloned();
    let session_file_items = session_file_items_result.unwrap_or_default();
    let items = if session_file_items.is_empty() {
        turn_items.clone()
    } else {
        session_file_items.clone()
    };
    Ok(json!({
        "caller": identity_view(&caller, "caller", Some(&caller)),
        "target": identity_view(&target, &target_handle_in_context(&caller, &target), Some(&caller)),
        "source": "app_server",
        "partial": false,
        "limit": limit,
        "items": items,
        "turnItems": turn_items,
        "sessionFileItems": session_file_items,
        "sessionFileError": session_file_error,
        "thread": thread.raw,
        "store": null
    }))
}

fn read_caller(caller_thread_id: Option<&str>) -> Result<ThreadInfo, String> {
    let Some(thread_id) = caller_thread_id else {
        return Err("Agent Mail could not bind the caller thread; CODEX_THREAD_ID or MCP thread metadata is required".to_string());
    };
    read_thread(thread_id, false)
}

fn team_for(caller: &ThreadInfo) -> Result<Team, String> {
    if let Some(parent_id) = caller.parent_thread_id.as_deref() {
        let main = read_thread(parent_id, false)?;
        let subagents = child_threads(&main.id)?;
        Ok(Team { main, subagents })
    } else {
        let subagents = child_threads(&caller.id)?;
        Ok(Team {
            main: caller.clone(),
            subagents,
        })
    }
}

fn repo_teams(cwd: Option<&str>) -> Result<Vec<RepoTeam>, String> {
    let mut main_threads = list_main_threads(cwd)?;
    main_threads.sort_by(|left, right| {
        right
            .updated_at
            .cmp(&left.updated_at)
            .then_with(|| right.created_at.cmp(&left.created_at))
            .then_with(|| left.id.cmp(&right.id))
    });
    let subagents = list_subagent_threads(cwd)?;
    Ok(main_threads
        .into_iter()
        .enumerate()
        .map(|(index, main)| {
            let mut children: Vec<ThreadInfo> = subagents
                .iter()
                .filter(|thread| thread.parent_thread_id.as_deref() == Some(main.id.as_str()))
                .cloned()
                .collect();
            children.sort_by(|left, right| {
                left.created_at
                    .cmp(&right.created_at)
                    .then_with(|| left.id.cmp(&right.id))
            });
            RepoTeam {
                index: index + 1,
                main,
                subagents: children,
            }
        })
        .collect())
}

fn resolve_target(caller: &ThreadInfo, target: &str) -> Result<ThreadInfo, String> {
    if looks_like_thread_id(target) {
        if let Ok(thread) = read_thread(target, false) {
            return Ok(thread);
        }
    }

    if let Some(rest) = target.strip_prefix("repo-team:") {
        let (index_text, inner) = rest
            .split_once('/')
            .ok_or_else(|| format!("target `{target}` must include /main or /subagent:N"))?;
        let index: usize = index_text
            .parse()
            .map_err(|_| format!("target `{target}` has an invalid repo-team index"))?;
        let teams = repo_teams(caller.cwd.as_deref())?;
        let team = teams
            .into_iter()
            .find(|team| team.index == index)
            .ok_or_else(|| format!("repo-team:{index} was not found"))?;
        return resolve_in_team(&team.main, &team.subagents, inner);
    }

    let team = team_for(caller)?;
    resolve_in_team(&team.main, &team.subagents, target)
}

fn resolve_in_team(
    main: &ThreadInfo,
    subagents: &[ThreadInfo],
    target: &str,
) -> Result<ThreadInfo, String> {
    if target == "main" {
        return Ok(main.clone());
    }
    if target == "subagent" {
        let open = subagents;
        return match open.len() {
            1 => Ok(open[0].clone()),
            0 => Err("target `subagent` was not found".to_string()),
            _ => Err(format!(
                "target `subagent` is ambiguous; use one of: {}",
                subagents
                    .iter()
                    .enumerate()
                    .map(|(idx, thread)| format!(
                        "subagent:{} ({})",
                        idx + 1,
                        thread.display_name()
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            )),
        };
    }
    if let Some(index_text) = target.strip_prefix("subagent:") {
        let index: usize = index_text
            .parse()
            .map_err(|_| format!("target `{target}` has an invalid subagent index"))?;
        return subagents
            .get(index.saturating_sub(1))
            .cloned()
            .ok_or_else(|| format!("target `{target}` was not found"));
    }

    let mut matches: Vec<ThreadInfo> = std::iter::once(main)
        .chain(subagents.iter())
        .filter(|thread| {
            thread.id == target
                || thread.agent_nickname.as_deref() == Some(target)
                || thread.name.as_deref() == Some(target)
                || thread.agent_role.as_deref() == target.strip_prefix("role:")
                || thread
                    .agent_nickname
                    .as_ref()
                    .zip(thread.agent_role.as_ref())
                    .map(|(name, role)| format!("{name}:{role}") == target)
                    .unwrap_or(false)
        })
        .cloned()
        .collect();
    match matches.len() {
        1 => Ok(matches.remove(0)),
        0 => Err(format!("target `{target}` was not found")),
        _ => Err(format!(
            "target `{target}` is ambiguous; use a thread id or one of: {}",
            matches
                .iter()
                .map(|thread| thread.id.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        )),
    }
}

fn list_main_threads(cwd: Option<&str>) -> Result<Vec<ThreadInfo>, String> {
    let params = match cwd {
        Some(cwd) => json!({
            "limit": 500,
            "archived": false,
            "useStateDbOnly": false,
            "cwd": cwd
        }),
        None => json!({
            "limit": 500,
            "archived": false,
            "useStateDbOnly": false
        }),
    };
    let value = app_server_request("thread/list", params)?;
    let threads = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "thread/list response did not contain data[]".to_string())?;
    Ok(threads
        .iter()
        .filter_map(thread_from_value)
        .filter(|thread| !thread.is_subagent())
        .collect())
}

fn list_subagent_threads(cwd: Option<&str>) -> Result<Vec<ThreadInfo>, String> {
    let source_kinds = [
        "subAgent",
        "subAgentThreadSpawn",
        "subAgentReview",
        "subAgentCompact",
        "subAgentOther",
    ];
    let params = match cwd {
        Some(cwd) => json!({
            "limit": 500,
            "sourceKinds": source_kinds,
            "archived": false,
            "useStateDbOnly": false,
            "cwd": cwd
        }),
        None => json!({
            "limit": 500,
            "sourceKinds": source_kinds,
            "archived": false,
            "useStateDbOnly": false
        }),
    };
    let value = app_server_request("thread/list", params)?;
    let threads = value
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| "thread/list response did not contain data[]".to_string())?;
    Ok(threads.iter().filter_map(thread_from_value).collect())
}

fn child_threads(parent_thread_id: &str) -> Result<Vec<ThreadInfo>, String> {
    let mut children: Vec<ThreadInfo> = list_subagent_threads(None)?
        .into_iter()
        .filter(|thread| thread.parent_thread_id.as_deref() == Some(parent_thread_id))
        .collect();
    children.sort_by(|left, right| {
        left.created_at
            .cmp(&right.created_at)
            .then_with(|| left.id.cmp(&right.id))
    });
    Ok(children)
}

fn read_thread(thread_id: &str, include_turns: bool) -> Result<ThreadInfo, String> {
    let response = app_server_request(
        "thread/read",
        json!({
            "threadId": thread_id,
            "includeTurns": include_turns
        }),
    )?;
    let thread = response
        .get("thread")
        .ok_or_else(|| "thread/read response did not contain thread".to_string())?;
    thread_from_value(thread).ok_or_else(|| "thread/read returned a malformed thread".to_string())
}

fn inject_user_message(thread_id: &str, message_id: &str, body: &str) -> Result<bool, String> {
    let params = json!({
        "threadId": thread_id,
        "items": [
            {
                "type": "message",
                "id": message_id,
                "role": "user",
                "content": [
                    {
                        "type": "input_text",
                        "text": body
                    }
                ]
            }
        ]
    });
    match app_server_request("thread/inject_items", params.clone()) {
        Ok(_) => Ok(false),
        Err(err) if is_thread_not_found(&err) => {
            resume_and_inject_user_message(thread_id, params).map_err(|fallback_err| {
                format!(
                    "thread/inject_items target was not materialized; thread/resume plus retry failed: {fallback_err}"
                )
            })?;
            Ok(true)
        }
        Err(err) => Err(err),
    }
}

fn resume_and_inject_user_message(thread_id: &str, inject_params: Value) -> Result<(), String> {
    let responses = app_server_requests(vec![
        (
            2,
            "thread/resume".to_string(),
            json!({
                "threadId": thread_id,
                "serviceTier": "flex"
            }),
        ),
        (3, "thread/inject_items".to_string(), inject_params),
    ])?;
    response_result(
        responses
            .get(&2)
            .ok_or_else(|| "thread/resume response was missing".to_string())?,
    )
    .map_err(|err| format!("thread/resume failed: {err}"))?;
    response_result(
        responses
            .get(&3)
            .ok_or_else(|| "thread/inject_items retry response was missing".to_string())?,
    )
    .map_err(|err| format!("thread/inject_items retry failed: {err}"))?;
    Ok(())
}

fn is_thread_not_found(message: &str) -> bool {
    message.to_ascii_lowercase().contains("thread not found")
}

fn app_server_request(method: &str, params: Value) -> Result<Value, String> {
    let responses = app_server_requests(vec![(2, method.to_string(), params)])?;
    response_result(
        responses
            .get(&2)
            .ok_or_else(|| format!("{method} response was missing"))?,
    )
}

fn app_server_requests(
    requests: Vec<(i64, String, Value)>,
) -> Result<BTreeMap<i64, Value>, String> {
    let initialize = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "clientInfo": {
                "name": "agent-mail-adapter",
                "title": null,
                "version": env!("CARGO_PKG_VERSION")
            },
            "capabilities": {
                "experimentalApi": true
            }
        }
    })
    .to_string();
    let requests: Vec<String> = requests
        .into_iter()
        .map(|(id, method, params)| {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "method": method,
                "params": params
            })
            .to_string()
        })
        .collect();

    let mut child = ProcessCommand::new("codex")
        .args([
            "app-server",
            "-c",
            "service_tier=\"flex\"",
            "--listen",
            "stdio://",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|err| format!("start codex app-server: {err}"))?;

    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{initialize}")
            .map_err(|err| format!("write app-server initialize: {err}"))?;
        stdin
            .flush()
            .map_err(|err| format!("flush app-server initialize: {err}"))?;
        sleep(Duration::from_millis(200));
        for request in requests {
            writeln!(stdin, "{request}")
                .map_err(|err| format!("write app-server request: {err}"))?;
            stdin
                .flush()
                .map_err(|err| format!("flush app-server request: {err}"))?;
            sleep(Duration::from_millis(250));
        }
        sleep(Duration::from_millis(700));
    }
    let output = child
        .wait_with_output()
        .map_err(|err| format!("wait for app-server: {err}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() && stdout.trim().is_empty() {
        return Err(format!("codex app-server failed: {}", stderr.trim()));
    }

    let mut responses = BTreeMap::new();
    for value in parse_jsonrpc_output(&stdout) {
        if let Some(id) = value.get("id").and_then(Value::as_i64) {
            responses.insert(id, value);
        }
    }
    if responses.is_empty() {
        return Err(format!(
            "codex app-server returned no JSON-RPC responses; stderr: {}",
            stderr.trim()
        ));
    }
    Ok(responses)
}

fn response_result(response: &Value) -> Result<Value, String> {
    if let Some(error) = response.get("error") {
        return Err(error
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| error.to_string()));
    }
    response
        .get("result")
        .cloned()
        .ok_or_else(|| "JSON-RPC response did not contain result".to_string())
}

fn parse_jsonrpc_output(output: &str) -> Vec<Value> {
    output
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            if line.starts_with('{') {
                serde_json::from_str(line).ok()
            } else {
                None
            }
        })
        .collect()
}

fn thread_from_value(value: &Value) -> Option<ThreadInfo> {
    let id = extract_string(value, &["id", "thread_id", "threadId"])?;
    let source = value.get("source").cloned().unwrap_or(Value::Null);
    Some(ThreadInfo {
        id,
        name: extract_string(value, &["name", "title"]),
        status: status_string(value.get("status")),
        cwd: extract_string(value, &["cwd", "currentWorkingDirectory", "working_dir"]),
        created_at: extract_string(value, &["createdAt", "created_at"])
            .or_else(|| value.get("createdAt").map(Value::to_string)),
        updated_at: extract_string(value, &["updatedAt", "updated_at"])
            .or_else(|| value.get("updatedAt").map(Value::to_string)),
        agent_nickname: extract_string(value, &["agentNickname", "agent_nickname"]),
        agent_role: extract_string(
            value,
            &["agentRole", "agent_role", "agentType", "agent_type"],
        ),
        parent_thread_id: parent_thread_id(value),
        source,
        raw: value.clone(),
    })
}

fn status_string(value: Option<&Value>) -> Option<String> {
    let value = value?;
    value
        .as_str()
        .map(str::to_string)
        .or_else(|| extract_string(value, &["type", "state"]))
        .or_else(|| Some(value.to_string()))
}

fn parent_thread_id(value: &Value) -> Option<String> {
    extract_string(value, &["parent_thread_id", "parentThreadId"]).or_else(|| {
        value.get("source").and_then(|source| {
            source
                .get("subAgent")
                .and_then(|subagent| subagent.get("thread_spawn"))
                .and_then(|spawn| extract_string(spawn, &["parent_thread_id", "parentThreadId"]))
                .or_else(|| {
                    source
                        .get("sub_agent")
                        .and_then(|subagent| subagent.get("thread_spawn"))
                        .and_then(|spawn| {
                            extract_string(spawn, &["parent_thread_id", "parentThreadId"])
                        })
                })
        })
    })
}

fn source_mentions_subagent(source: &Value) -> bool {
    source.to_string().to_ascii_lowercase().contains("subagent")
}

fn team_view(team: &Team, caller: &ThreadInfo) -> Value {
    let subagents: Vec<Value> = team
        .subagents
        .iter()
        .enumerate()
        .map(|(index, thread)| {
            identity_view(thread, &format!("subagent:{}", index + 1), Some(caller))
        })
        .collect();
    json!({
        "caller": identity_view(caller, &target_handle_in_context(caller, caller), Some(caller)),
        "ownAgent": identity_view(caller, &target_handle_in_context(caller, caller), Some(caller)),
        "teamId": team.main.id,
        "main": identity_view(&team.main, "main", Some(caller)),
        "subagents": subagents,
        "source": "app_server",
        "store": null
    })
}

fn repo_team_view(team: &RepoTeam) -> Value {
    json!({
        "handle": format!("repo-team:{}", team.index),
        "teamId": team.main.id,
        "main": identity_view(&team.main, &format!("repo-team:{}/main", team.index), None),
        "subagents": team.subagents.iter().enumerate().map(|(idx, thread)| {
            identity_view(thread, &format!("repo-team:{}/subagent:{}", team.index, idx + 1), None)
        }).collect::<Vec<_>>(),
        "repoId": team.main.cwd,
        "workspace": team.main.cwd.as_deref().and_then(workspace_name),
        "cwd": team.main.cwd,
        "state": team.main.status,
        "source": "app_server",
        "freshness": "live",
        "cached": false,
        "stale": false,
        "updatedAt": team.main.updated_at
    })
}

fn identity_view(thread: &ThreadInfo, handle: &str, caller: Option<&ThreadInfo>) -> Value {
    let is_caller = caller.map(|caller| caller.id == thread.id).unwrap_or(false);
    json!({
        "handle": handle,
        "id": thread.id,
        "kind": if thread.is_subagent() { "subagent" } else { "main" },
        "name": thread.display_name(),
        "role": thread.agent_role,
        "state": thread.status.clone().unwrap_or_else(|| "unknown".to_string()),
        "isCaller": is_caller,
        "canWrite": !thread.is_subagent(),
        "canRead": true,
        "threadId": thread.id,
        "parentThreadId": thread.parent_thread_id,
        "teamId": root_thread_id(thread).unwrap_or_else(|_| thread.id.clone()),
        "repoId": thread.cwd,
        "workspace": thread.cwd.as_deref().and_then(workspace_name),
        "cwd": thread.cwd,
        "source": "app_server",
        "freshness": "live",
        "cached": false,
        "stale": false,
        "updatedAt": thread.updated_at,
        "rawSource": thread.source
    })
}

fn target_handle_in_context(caller: &ThreadInfo, target: &ThreadInfo) -> String {
    if caller.id == target.id {
        return if caller.is_subagent() {
            "subagent:self".to_string()
        } else {
            "main".to_string()
        };
    }
    if !target.is_subagent() {
        return "main".to_string();
    }
    if let Ok(team) = team_for(caller) {
        if let Some(index) = team
            .subagents
            .iter()
            .position(|thread| thread.id == target.id)
        {
            return format!("subagent:{}", index + 1);
        }
    }
    target.id.clone()
}

fn root_thread_id(thread: &ThreadInfo) -> Result<String, String> {
    if let Some(parent) = thread.parent_thread_id.as_deref() {
        Ok(parent.to_string())
    } else {
        Ok(thread.id.clone())
    }
}

fn compact_thread_items(thread: &Value, limit: usize) -> Vec<Value> {
    let Some(turns) = thread.get("turns").and_then(Value::as_array) else {
        return Vec::new();
    };
    let mut items = Vec::new();
    for turn in turns.iter().rev() {
        let turn_id = extract_string(turn, &["id"]);
        if let Some(turn_items) = turn.get("items").and_then(Value::as_array) {
            for item in turn_items.iter().rev() {
                items.push(json!({
                    "turnId": turn_id,
                    "type": item.get("type").and_then(Value::as_str),
                    "text": item_text(item),
                    "item": item
                }));
                if items.len() >= limit {
                    items.reverse();
                    return items;
                }
            }
        }
    }
    items.reverse();
    items
}

fn compact_session_file_items(thread: &Value, limit: usize) -> Result<Vec<Value>, String> {
    let Some(path) = extract_string(thread, &["path"]) else {
        return Ok(Vec::new());
    };
    let content = fs::read_to_string(&path)
        .map_err(|err| format!("read session transcript {path}: {err}"))?;
    let mut items = Vec::new();
    for line in content.lines() {
        let Ok(record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record.get("type").and_then(Value::as_str) != Some("response_item") {
            continue;
        }
        let Some(payload) = record.get("payload") else {
            continue;
        };
        if payload.get("type").and_then(Value::as_str) == Some("reasoning") {
            continue;
        }
        items.push(json!({
            "source": "session_jsonl",
            "timestamp": record.get("timestamp").and_then(Value::as_str),
            "type": payload.get("type").and_then(Value::as_str),
            "role": payload.get("role").and_then(Value::as_str),
            "text": item_text(payload),
            "item": payload
        }));
    }
    if items.len() > limit {
        Ok(items.split_off(items.len() - limit))
    } else {
        Ok(items)
    }
}

fn item_text(item: &Value) -> Option<String> {
    if let Some(text) = extract_string(item, &["text", "message", "body"]) {
        return Some(single_line(&text, 500));
    }
    if let Some(content) = item.get("content").and_then(Value::as_array) {
        let mut parts = Vec::new();
        for part in content {
            if let Some(text) = extract_string(part, &["text"]) {
                parts.push(text);
            }
        }
        if !parts.is_empty() {
            return Some(single_line(&parts.join(" "), 500));
        }
    }
    None
}

fn metadata_prefixed_body(caller: &ThreadInfo, body: &str) -> String {
    format!(
        "FROM_THREAD_ID={} FROM_THREAD_NAME=\"{}\" FROM_AGENT_NAME={}\n{}",
        caller.id,
        caller.name.clone().unwrap_or_default().replace('"', "'"),
        caller.display_name().replace(char::is_whitespace, "_"),
        body
    )
}

fn value_contains_text(value: &Value, needle: &str) -> bool {
    match value {
        Value::String(text) => text.contains(needle),
        Value::Array(values) => values
            .iter()
            .any(|value| value_contains_text(value, needle)),
        Value::Object(map) => map.values().any(|value| value_contains_text(value, needle)),
        _ => false,
    }
}

fn thread_contains_text(thread: &ThreadInfo, needle: &str) -> bool {
    value_contains_text(&thread.raw, needle)
        || session_file_contains_text(&thread.raw, needle).unwrap_or(false)
}

fn confirm_thread_contains_text(target: &ThreadInfo, needle: &str) -> bool {
    for delay_ms in [0, 250, 500, 1000, 2000, 4000] {
        if delay_ms > 0 {
            sleep(Duration::from_millis(delay_ms));
        }
        if thread_contains_text(target, needle) {
            return true;
        }
    }
    if let Ok(thread) = read_thread(&target.id, true) {
        if thread_contains_text(&thread, needle) {
            return true;
        }
    }
    false
}

fn session_file_contains_text(thread: &Value, needle: &str) -> Result<bool, String> {
    let Some(path) = extract_string(thread, &["path"]) else {
        return Ok(false);
    };
    let content = fs::read_to_string(&path)
        .map_err(|err| format!("read session transcript {path}: {err}"))?;
    Ok(content.contains(needle))
}

fn caller_thread_id(params: &Value) -> Option<String> {
    metadata_string(
        params,
        &[
            "thread_id",
            "threadId",
            "codex_thread_id",
            "codexThreadId",
            "session_id",
            "sessionId",
        ],
    )
}

fn metadata_string(params: &Value, keys: &[&str]) -> Option<String> {
    extract_string(params, keys)
        .or_else(|| {
            params
                .get("_meta")
                .and_then(|meta| extract_string(meta, keys))
        })
        .or_else(|| {
            params
                .get("metadata")
                .and_then(|meta| extract_string(meta, keys))
        })
}

fn env_session_id() -> Option<String> {
    [
        "CODEX_THREAD_ID",
        "CODEX_SESSION_ID",
        "CODEX_CONVERSATION_ID",
        "AGENT_MAIL_SESSION_ID",
    ]
    .iter()
    .find_map(|name| env::var(name).ok())
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty())
}

fn hook_input_is_subagent(event: HookEvent, input: &Value) -> bool {
    matches!(event, HookEvent::SubagentStart | HookEvent::SubagentStop)
        || parent_thread_id(input).is_some()
        || extract_string(input, &["thread_source", "threadSource"])
            .map(|value| value.eq_ignore_ascii_case("subagent"))
            .unwrap_or(false)
}

fn hook_event_name(event: HookEvent) -> &'static str {
    match event {
        HookEvent::SessionStart => "SessionStart",
        HookEvent::SubagentStart => "SubagentStart",
        HookEvent::UserPromptSubmit => "UserPromptSubmit",
        HookEvent::PostToolUse => "PostToolUse",
        HookEvent::Stop => "Stop",
        HookEvent::SubagentStop => "SubagentStop",
    }
}

fn required_string(value: &Value, key: &str) -> Result<String, String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or_else(|| format!("missing required string `{key}`"))
}

fn optional_u32(value: &Value, key: &str) -> Option<u32> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as u32)
}

fn reject_unknown(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let Some(object) = value.as_object() else {
        return Ok(());
    };
    let allowed: BTreeSet<&str> = allowed.iter().copied().collect();
    for key in object.keys() {
        if !allowed.contains(key.as_str()) {
            return Err(format!("unknown argument `{key}`"));
        }
    }
    Ok(())
}

fn extract_string(value: &Value, keys: &[&str]) -> Option<String> {
    keys.iter()
        .find_map(|key| {
            value.get(*key).and_then(|value| {
                value
                    .as_str()
                    .map(str::to_string)
                    .or_else(|| value.as_i64().map(|value| value.to_string()))
                    .or_else(|| value.as_u64().map(|value| value.to_string()))
            })
        })
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn workspace_name(cwd: &str) -> Option<String> {
    Path::new(cwd)
        .file_name()
        .map(|name| name.to_string_lossy().to_string())
}

fn current_cwd() -> Option<String> {
    env::current_dir()
        .ok()
        .map(|path| path.display().to_string())
}

fn looks_like_thread_id(value: &str) -> bool {
    value.starts_with("019") || value.starts_with("thr_") || value.contains('-')
}

fn stable_id(seed: &str) -> String {
    let mut hash: u64 = 1469598103934665603;
    for byte in seed.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(1099511628211);
    }
    format!("{hash:016x}")
}

fn single_line(value: &str, max_chars: usize) -> String {
    let value = value.split_whitespace().collect::<Vec<_>>().join(" ");
    if value.chars().count() > max_chars {
        let mut out = value.chars().take(max_chars).collect::<String>();
        out.push_str("...");
        out
    } else {
        value
    }
}

fn read_stdin_json() -> Option<Value> {
    let mut buffer = String::new();
    io::stdin().read_to_string(&mut buffer).ok()?;
    serde_json::from_str(buffer.trim()).ok()
}

fn read_json_rpc_message<R: BufRead>(input: &mut R) -> Result<Option<Value>, String> {
    let mut line = String::new();
    loop {
        line.clear();
        let bytes = input.read_line(&mut line).map_err(|err| err.to_string())?;
        if bytes == 0 {
            return Ok(None);
        }
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with('{') {
            return serde_json::from_str(trimmed)
                .map(Some)
                .map_err(|err| err.to_string());
        }
        if let Some(length_text) = trimmed.strip_prefix("Content-Length:") {
            let length: usize = length_text
                .trim()
                .parse()
                .map_err(|err| format!("invalid Content-Length: {err}"))?;
            line.clear();
            input.read_line(&mut line).map_err(|err| err.to_string())?;
            let mut body = vec![0_u8; length];
            input.read_exact(&mut body).map_err(|err| err.to_string())?;
            return serde_json::from_slice(&body)
                .map(Some)
                .map_err(|err| err.to_string());
        }
    }
}

fn tool_descriptors() -> Value {
    json!([
        {
            "name": "my_team",
            "description": "List the caller's real Codex main/subagent team using thread/read and thread/list. No Agent Mail store is used.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "closed": { "type": "boolean", "default": false }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "repo_teams",
            "description": "List real Codex main-agent teams in this repo/workspace using thread/list, with direct subagents grouped by parent thread id.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "closed": { "type": "boolean", "default": false }
                },
                "additionalProperties": false
            }
        },
        {
            "name": "write",
            "description": "Append non-terminating mail to another Codex agent's real thread history with thread/inject_items. This is not turn-addressed and does not use a plugin mailbox.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "to": { "type": "string" },
                    "body": { "type": "string" },
                    "interrupt": { "type": "boolean", "default": false },
                    "forwardNext": { "type": "integer", "minimum": 0, "maximum": 5, "default": 0 },
                    "requireReply": { "type": "boolean", "default": false }
                },
                "required": ["to", "body"],
                "additionalProperties": false
            }
        },
        {
            "name": "read",
            "description": "Read real Codex thread context using thread/read plus the target session transcript path when available. No Agent Mail store is used.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "target": { "type": "string" },
                    "limit": { "type": "integer", "minimum": 1, "maximum": 100, "default": 10 },
                    "since": { "type": "string" },
                    "until": { "type": "string" }
                },
                "required": ["target"],
                "additionalProperties": false
            }
        }
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn thread(id: &str, name: &str, parent: Option<&str>) -> ThreadInfo {
        ThreadInfo {
            id: id.to_string(),
            name: Some(name.to_string()),
            status: Some("open".to_string()),
            cwd: Some("/repo".to_string()),
            created_at: None,
            updated_at: None,
            agent_nickname: Some(name.to_string()),
            agent_role: None,
            parent_thread_id: parent.map(str::to_string),
            source: Value::Null,
            raw: json!({ "id": id, "name": name }),
        }
    }

    #[test]
    fn parses_subagent_parent_from_thread_source() {
        let value = json!({
            "id": "child",
            "name": "child",
            "source": {
                "subAgent": {
                    "thread_spawn": {
                        "parent_thread_id": "main"
                    }
                }
            }
        });
        let parsed = thread_from_value(&value).unwrap();
        assert_eq!(parsed.parent_thread_id.as_deref(), Some("main"));
        assert!(parsed.is_subagent());
    }

    #[test]
    fn resolves_team_handles_without_store() {
        let main = thread("main", "Main", None);
        let child = thread("child", "Taste", Some("main"));
        let resolved = resolve_in_team(&main, &[child], "subagent:1").unwrap();
        assert_eq!(resolved.id, "child");
    }

    #[test]
    fn metadata_prefix_includes_sender_thread_id() {
        let caller = thread("main-thread", "Main Agent", None);
        let body = metadata_prefixed_body(&caller, "hello");
        assert!(body.starts_with("FROM_THREAD_ID=main-thread"));
        assert!(body.contains("\nhello"));
    }

    #[test]
    fn hook_copy_is_role_aware() {
        let payload = json!({
            "parent_thread_id": "main"
        });
        assert!(hook_input_is_subagent(
            HookEvent::UserPromptSubmit,
            &payload
        ));
        assert!(!hook_input_is_subagent(
            HookEvent::SessionStart,
            &Value::Null
        ));
    }

    #[test]
    fn parses_session_file_response_items() {
        let path = env::temp_dir().join(format!(
            "agent-mail-session-items-{}.jsonl",
            stable_id("session-file-test")
        ));
        fs::write(
            &path,
            r#"{"timestamp":"2026-06-09T00:00:00Z","type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"hello injected transcript"}]}}
{"timestamp":"2026-06-09T00:00:01Z","type":"response_item","payload":{"type":"reasoning","summary":[]}}
"#,
        )
        .unwrap();
        let thread = json!({ "path": path.to_string_lossy() });
        let items = compact_session_file_items(&thread, 10).unwrap();
        fs::remove_file(&path).ok();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["role"], "user");
        assert_eq!(items[0]["text"], "hello injected transcript");
    }

    #[test]
    fn parses_line_json_rpc_message() {
        let input = br#"{"jsonrpc":"2.0","id":1,"method":"tools/list"}"#;
        let mut reader = io::BufReader::new(&input[..]);
        let message = read_json_rpc_message(&mut reader).unwrap().unwrap();
        assert_eq!(message["method"], "tools/list");
    }
}
