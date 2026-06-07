use std::collections::HashMap;
use std::collections::HashSet;
use std::env;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::Context;
use anyhow::Result;
use anyhow::anyhow;
use chrono::DateTime;
use chrono::Utc;
use clap::Parser;
use clap::Subcommand;
use rusqlite::Connection;
use rusqlite::OpenFlags;
use rusqlite::OptionalExtension;
use rusqlite::params;

const STATE_DB_FILENAME: &str = "state_5.sqlite";

#[derive(Debug, Parser)]
#[command(name = "agent-mail")]
#[command(about = "Coordinate Codex main/subagent threads")]
struct Cli {
    #[arg(long, global = true, env = "AGENT_MAIL_STATE_DB")]
    state_db: Option<PathBuf>,
    #[arg(long, global = true, env = "AGENT_MAIL_SELF_THREAD_ID")]
    self_thread: Option<String>,
    #[arg(long, global = true, env = "AGENT_MAIL_PARENT_THREAD_ID")]
    parent_thread: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show this Codex main/subagent team.
    #[command(
        alias = "subagents",
        alias = "coworkers",
        alias = "contacts",
        alias = "ls"
    )]
    Team {
        #[arg(long, default_value_t = 100)]
        limit: u32,
        #[arg(long)]
        technical: bool,
    },
    /// Read host-native Agent Mail. Fails until Codex exposes agent_mail.read.
    Read {
        target: String,
        #[arg(long, default_value_t = 10)]
        limit: usize,
    },
    /// Write host-native queued Agent Mail. Fails until Codex exposes agent_mail.write.
    Write {
        target: String,
        message: String,
        #[arg(long)]
        interrupt: bool,
    },
    /// Hook entrypoints that inject model-visible Agent Mail context.
    #[command(hide = true)]
    Hook { event: HookEvent },
}

#[derive(Debug, Clone, clap::ValueEnum)]
enum HookEvent {
    SessionStart,
    SubagentStart,
    UserPromptSubmit,
}

#[derive(Debug, Clone)]
struct Scope {
    state_db: PathBuf,
    self_thread: Option<String>,
    parent_thread: Option<String>,
}

#[derive(Debug, Clone)]
struct AgentRecord {
    thread_id: String,
    parent_thread_id: Option<String>,
    edge_status: Option<String>,
    created_at: i64,
    updated_at: i64,
    title: String,
    agent_nickname: Option<String>,
    agent_role: Option<String>,
    agent_path: Option<String>,
    preview: String,
}

#[derive(Debug, Clone)]
struct Directory {
    current_family: Vec<AgentRecord>,
}

fn main() {
    if let Err(err) = run() {
        eprintln!("{err:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let scope = Scope {
        state_db: cli
            .state_db
            .or_else(default_state_db_path)
            .ok_or_else(|| anyhow!("Could not find CODEX_HOME or HOME for state DB lookup"))?,
        self_thread: cli.self_thread.or_else(|| env::var("CODEX_THREAD_ID").ok()),
        parent_thread: cli.parent_thread,
    };

    match cli.command {
        Command::Team { limit, technical } => {
            let directory = load_directory(&scope, limit)?;
            print_team(&directory.current_family, technical);
        }
        Command::Read { target, limit: _ } => {
            let directory = load_directory(&scope, 500)?;
            let target = resolve_target(&directory, &target)?;
            return Err(native_agent_mail_required("read", target));
        }
        Command::Write {
            target,
            message: _,
            interrupt: _,
        } => {
            let directory = load_directory(&scope, 500)?;
            let target = resolve_target(&directory, &target)?;
            return Err(native_agent_mail_required("write", target));
        }
        Command::Hook { event } => print_hook_context(event)?,
    }
    Ok(())
}

fn default_state_db_path() -> Option<PathBuf> {
    if let Some(value) = env::var_os("CODEX_STATE_DB_PATH") {
        return Some(PathBuf::from(value));
    }
    env::var_os("CODEX_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".codex")))
        .map(|codex_home| codex_home.join(STATE_DB_FILENAME))
}

fn load_directory(scope: &Scope, limit: u32) -> Result<Directory> {
    let conn = open_state_db(&scope.state_db)?;
    let current_family = load_current_family(&conn, scope, limit)?;
    Ok(Directory { current_family })
}

fn open_state_db(path: &Path) -> Result<Connection> {
    if !path.exists() {
        return Err(anyhow!(
            "Codex state DB was not found at `{}`",
            path.display()
        ));
    }
    let conn = Connection::open_with_flags(
        path,
        OpenFlags::SQLITE_OPEN_READ_ONLY
            | OpenFlags::SQLITE_OPEN_URI
            | OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .with_context(|| format!("failed to open Codex state DB `{}`", path.display()))?;
    conn.busy_timeout(Duration::from_millis(50))?;
    Ok(conn)
}

fn load_current_family(conn: &Connection, scope: &Scope, limit: u32) -> Result<Vec<AgentRecord>> {
    let Some(root) = resolve_current_root(conn, scope)? else {
        return Ok(Vec::new());
    };
    load_family_threads(conn, &root, limit)
}

fn resolve_current_root(conn: &Connection, scope: &Scope) -> Result<Option<String>> {
    if let Some(self_thread) = scope.self_thread.as_deref()
        && thread_exists(conn, self_thread)?
    {
        return Ok(Some(ascend_to_root(conn, self_thread)?));
    }
    if let Some(parent_thread) = scope.parent_thread.as_deref()
        && thread_exists(conn, parent_thread)?
    {
        return Ok(Some(ascend_to_root(conn, parent_thread)?));
    }
    active_family_roots(conn, 1).map(|mut roots| roots.pop())
}

fn thread_exists(conn: &Connection, thread_id: &str) -> Result<bool> {
    let exists = conn
        .query_row(
            "SELECT 1 FROM threads WHERE id = ? LIMIT 1",
            params![thread_id],
            |_| Ok(()),
        )
        .optional()?
        .is_some();
    Ok(exists)
}

fn ascend_to_root(conn: &Connection, thread_id: &str) -> Result<String> {
    let mut current = thread_id.to_string();
    let mut seen = HashSet::new();
    loop {
        if !seen.insert(current.clone()) {
            return Err(anyhow!("native spawn graph cycle at `{current}`"));
        }
        let parent = conn
            .query_row(
                "SELECT parent_thread_id FROM thread_spawn_edges WHERE child_thread_id = ?",
                params![current],
                |row| row.get::<_, String>(0),
            )
            .optional()?;
        let Some(parent) = parent else {
            return Ok(current);
        };
        current = parent;
    }
}

fn active_family_roots(conn: &Connection, limit: u32) -> Result<Vec<String>> {
    let mut stmt = conn.prepare(
        r#"
SELECT edge.parent_thread_id, edge.child_thread_id
FROM thread_spawn_edges AS edge
JOIN threads AS child ON child.id = edge.child_thread_id
WHERE edge.status = 'open'
  AND child.archived = 0
ORDER BY child.updated_at DESC, child.id DESC
LIMIT ?1
        "#,
    )?;
    let rows = stmt.query_map(params![limit.max(1)], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
    })?;
    let mut roots = Vec::new();
    let mut seen = HashSet::new();
    for row in rows {
        let (parent_id, child_id) = row?;
        let root = ascend_to_root(conn, &parent_id).or_else(|_| ascend_to_root(conn, &child_id))?;
        if seen.insert(root.clone()) {
            roots.push(root);
        }
    }
    Ok(roots)
}

fn load_family_threads(conn: &Connection, root: &str, limit: u32) -> Result<Vec<AgentRecord>> {
    let mut stmt = conn.prepare(
        r#"
WITH RECURSIVE family(id, parent_thread_id, edge_status, depth) AS (
    SELECT ?1, NULL, NULL, 0
    UNION ALL
    SELECT edge.child_thread_id, edge.parent_thread_id, edge.status, family.depth + 1
    FROM thread_spawn_edges AS edge
    JOIN family ON edge.parent_thread_id = family.id
)
SELECT
    threads.id,
    family.parent_thread_id,
    family.edge_status,
    threads.created_at,
    threads.updated_at,
    threads.title,
    threads.agent_nickname,
    threads.agent_role,
    threads.agent_path,
    threads.preview
FROM family
JOIN threads ON threads.id = family.id
WHERE threads.archived = 0
ORDER BY family.depth ASC, threads.created_at ASC, threads.id ASC
LIMIT ?2
        "#,
    )?;
    let rows = stmt.query_map(params![root, limit.max(1)], |row| {
        let title: String = row.get(5)?;
        let preview: String = row.get(9)?;
        Ok(AgentRecord {
            thread_id: row.get(0)?,
            parent_thread_id: row.get(1)?,
            edge_status: row.get(2)?,
            created_at: row.get(3)?,
            updated_at: row.get(4)?,
            title: title.clone(),
            agent_nickname: row.get(6)?,
            agent_role: row.get(7)?,
            agent_path: row.get(8)?,
            preview: if preview.trim().is_empty() {
                title
            } else {
                preview
            },
        })
    })?;
    let mut agents = Vec::new();
    for row in rows {
        agents.push(row?);
    }
    Ok(agents)
}

fn print_team(family: &[AgentRecord], technical: bool) {
    println!("Team");
    println!();
    if family.is_empty() {
        println!("No current Codex main/subagent family found.");
        return;
    }
    if technical {
        println!(
            "{:<14} {:<24} {:<8} {:<20} {:<20} id",
            "handle", "name", "state", "created", "updated"
        );
    } else {
        println!("{:<14} {:<28} state", "handle", "name");
    }
    let aliases = family_aliases(family);
    let name_aliases = team_name_aliases(family);
    for agent in family {
        let display_label = name_aliases
            .get(&agent.thread_id)
            .cloned()
            .unwrap_or_else(|| display_name(agent));
        print_agent_row(alias_for(agent, &aliases), agent, technical, &display_label);
    }
}

fn print_agent_row(alias: String, agent: &AgentRecord, technical: bool, display_label: &str) {
    if technical {
        println!(
            "{:<14} {:<24} {:<8} created={} updated={} id={}",
            alias,
            truncate(display_label, 24),
            status_label(agent),
            format_epoch_utc(agent.created_at),
            format_epoch_utc(agent.updated_at),
            agent.thread_id
        );
    } else {
        println!(
            "{:<14} {:<28} {}",
            alias,
            truncate(display_label, 28),
            status_label(agent)
        );
    }
}

fn family_aliases(family: &[AgentRecord]) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    let mut subagent_index = 1;
    for agent in family {
        let alias = if agent.parent_thread_id.is_none() {
            "main".to_string()
        } else {
            let alias = format!("subagent:{subagent_index}");
            subagent_index += 1;
            alias
        };
        aliases.insert(agent.thread_id.clone(), alias);
    }
    aliases
}

fn alias_for(agent: &AgentRecord, aliases: &HashMap<String, String>) -> String {
    aliases
        .get(&agent.thread_id)
        .cloned()
        .unwrap_or_else(|| thread_handle(agent))
}

fn team_name_aliases(family: &[AgentRecord]) -> HashMap<String, String> {
    stable_value_aliases(family, None, |agent| Some(display_name(agent)))
}

fn team_friendly_aliases(family: &[AgentRecord]) -> HashMap<String, String> {
    let mut aliases = HashMap::new();
    for alias_set in [
        stable_value_aliases(family, None, |agent| Some(display_name(agent))),
        stable_value_aliases(family, Some("role"), |agent| agent.agent_role.clone()),
        stable_value_aliases(family, None, |agent| {
            let role = agent.agent_role.as_deref()?.trim();
            (!role.is_empty()).then(|| format!("{}:{role}", display_name(agent)))
        }),
    ] {
        for (thread_id, alias) in alias_set {
            aliases.insert(normalize(&alias), thread_id);
        }
    }
    aliases
}

fn stable_value_aliases<F>(
    family: &[AgentRecord],
    prefix: Option<&str>,
    value_for: F,
) -> HashMap<String, String>
where
    F: Fn(&AgentRecord) -> Option<String>,
{
    let values = family
        .iter()
        .filter(|agent| agent.parent_thread_id.is_some())
        .filter_map(|agent| {
            let value = value_for(agent)?;
            let value = value.trim();
            (!value.is_empty())
                .then(|| (agent.thread_id.clone(), value.to_string(), normalize(value)))
        })
        .collect::<Vec<_>>();
    let mut totals = HashMap::<String, usize>::new();
    for (_, _, key) in &values {
        *totals.entry(key.clone()).or_default() += 1;
    }
    let mut seen = HashMap::<String, usize>::new();
    let mut aliases = HashMap::new();
    for (thread_id, value, key) in values {
        let index = seen.entry(key.clone()).or_insert(0);
        *index += 1;
        let value = if totals.get(&key).copied().unwrap_or(0) > 1 {
            format!("{value}#{index}")
        } else {
            value
        };
        let alias = match prefix {
            Some(prefix) => format!("{prefix}:{value}"),
            None => value,
        };
        aliases.insert(thread_id, alias);
    }
    aliases
}

fn thread_handle(agent: &AgentRecord) -> String {
    let tail = agent
        .thread_id
        .chars()
        .rev()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .take(8)
        .collect::<String>()
        .chars()
        .rev()
        .collect::<String>();
    format!("thread:{tail}")
}

fn display_name(agent: &AgentRecord) -> String {
    agent
        .agent_nickname
        .as_deref()
        .or(agent.agent_role.as_deref())
        .or_else(|| non_empty(&agent.preview))
        .or_else(|| non_empty(&agent.title))
        .unwrap_or("Agent")
        .trim()
        .to_string()
}

fn status_label(agent: &AgentRecord) -> String {
    if agent.parent_thread_id.is_none() {
        "main".to_string()
    } else {
        agent
            .edge_status
            .as_deref()
            .filter(|status| !status.trim().is_empty())
            .unwrap_or("unknown")
            .to_string()
    }
}

fn search_terms(agent: &AgentRecord) -> Vec<String> {
    [
        Some(display_name(agent)),
        Some(thread_handle(agent)),
        Some(agent.thread_id.clone()),
        agent.agent_path.clone(),
        agent.agent_nickname.clone(),
        agent.agent_role.clone(),
        Some(agent.preview.clone()),
        Some(agent.title.clone()),
    ]
    .into_iter()
    .flatten()
    .filter(|value| !value.trim().is_empty())
    .collect()
}

fn resolve_target<'a>(directory: &'a Directory, target: &str) -> Result<&'a AgentRecord> {
    let target = target.trim();
    if target.is_empty() {
        return Err(anyhow!("empty target"));
    }

    let current_aliases = family_aliases(&directory.current_family);
    for agent in &directory.current_family {
        if current_aliases
            .get(&agent.thread_id)
            .is_some_and(|alias| alias == target)
        {
            return Ok(agent);
        }
    }

    if target == "subagent" {
        let subagents = directory
            .current_family
            .iter()
            .filter(|agent| agent.parent_thread_id.is_some())
            .collect::<Vec<_>>();
        match subagents.as_slice() {
            [agent] => return Ok(*agent),
            [] => {
                return Err(anyhow!(
                    "There are no subagents in this team.\nRun `agent-mail team`."
                ));
            }
            _ => {
                return Err(anyhow!(
                    "`subagent` is ambiguous because this team has more than one subagent.\nRun `agent-mail team` and use `subagent:N`."
                ));
            }
        }
    }

    if let Some(thread_id) =
        team_friendly_aliases(&directory.current_family).get(&normalize(target))
        && let Some(agent) = directory
            .current_family
            .iter()
            .find(|agent| &agent.thread_id == thread_id)
    {
        return Ok(agent);
    }

    let agents = directory_agents(directory);
    let matches = agents
        .into_iter()
        .filter(|agent| target_matches_agent(agent, target))
        .collect::<Vec<_>>();
    match matches.as_slice() {
        [agent] => Ok(*agent),
        [] => Err(anyhow!(
            "I couldn't find `{target}` in this main/subagent team.\nRun `agent-mail team`."
        )),
        _ => Err(anyhow!(
            "`{target}` matches more than one thread.\nRun `agent-mail team --technical` and use `main` or `subagent:N`."
        )),
    }
}

fn directory_agents(directory: &Directory) -> Vec<&AgentRecord> {
    directory.current_family.iter().collect()
}

fn target_matches_agent(agent: &AgentRecord, target: &str) -> bool {
    if target.starts_with("thread:") {
        let token = target.trim_start_matches("thread:");
        return thread_handle(agent) == target
            || agent.thread_id == token
            || agent.thread_id.starts_with(token)
            || agent.thread_id.ends_with(token);
    }
    if agent.thread_id == target || agent.thread_id.starts_with(target) {
        return true;
    }
    let target = normalize(target);
    if normalize(&thread_handle(agent)).contains(&target) {
        return true;
    }
    let exact_name = search_terms(agent)
        .iter()
        .any(|term| normalize(term) == target);
    exact_name
        || search_terms(agent)
            .iter()
            .any(|term| normalize(term).starts_with(&target))
}

fn native_agent_mail_required(action: &str, target: &AgentRecord) -> anyhow::Error {
    anyhow!(
        "Agent Mail `{action}` requires host-native `agent_mail/{action}` support.\n\
         Resolved target: {} ({})\n\
         This CLI will not write a local mailbox, inject transcript items, or claim GUI-native delivery.",
        display_name(target),
        thread_handle(target)
    )
}

fn print_hook_context(event: HookEvent) -> Result<()> {
    let context = match event {
        HookEvent::SessionStart => {
            "Agent Mail is native-GUI-first. Use `agent-mail team` to see this main/subagent team; use hosted `agent_mail.write/read` for real mail when Codex exposes them."
        }
        HookEvent::SubagentStart => {
            "You are reachable through Agent Mail when host-native `agent_mail` is available. Use `agent-mail team` to identify the main thread and peer subagents; do not claim CLI write/read delivered GUI mail without a host-native receipt."
        }
        HookEvent::UserPromptSubmit => {
            "Agent Mail reminder: `agent-mail team` shows this main/subagent team. Hosted `agent_mail.write` sends queued GUI mail when Codex exposes it."
        }
    };
    println!(
        "{}",
        serde_json::json!({
            "hookSpecificOutput": {
                "hookEventName": hook_event_name(&event),
                "additionalContext": context
            }
        })
    );
    Ok(())
}

fn hook_event_name(event: &HookEvent) -> &'static str {
    match event {
        HookEvent::SessionStart => "SessionStart",
        HookEvent::SubagentStart => "SubagentStart",
        HookEvent::UserPromptSubmit => "UserPromptSubmit",
    }
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn truncate(value: &str, max: usize) -> String {
    let value = value.trim();
    if value.chars().count() <= max {
        return value.to_string();
    }
    let keep = max.saturating_sub(1);
    format!("{}...", value.chars().take(keep).collect::<String>())
}

fn format_epoch_utc(seconds: i64) -> String {
    DateTime::<Utc>::from_timestamp(seconds, 0)
        .map(|timestamp| timestamp.format("%Y-%m-%dT%H:%M:%SZ").to_string())
        .unwrap_or_else(|| seconds.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(id: &str, parent: Option<&str>, name: &str, status: Option<&str>) -> AgentRecord {
        AgentRecord {
            thread_id: id.to_string(),
            parent_thread_id: parent.map(str::to_string),
            edge_status: status.map(str::to_string),
            created_at: 1780697400,
            updated_at: 1780697708,
            title: name.to_string(),
            agent_nickname: Some(name.to_string()),
            agent_role: None,
            agent_path: None,
            preview: name.to_string(),
        }
    }

    fn agent_with_role(
        id: &str,
        parent: Option<&str>,
        name: &str,
        role: &str,
        status: Option<&str>,
    ) -> AgentRecord {
        AgentRecord {
            agent_role: Some(role.to_string()),
            ..agent(id, parent, name, status)
        }
    }

    #[test]
    fn current_family_aliases_are_local() {
        let family = vec![
            agent("root", None, "Root", None),
            agent("child-a", Some("root"), "Dirac", Some("open")),
            agent("child-b", Some("root"), "Erdos", Some("open")),
        ];
        let aliases = family_aliases(&family);
        assert_eq!(aliases["root"], "main");
        assert_eq!(aliases["child-a"], "subagent:1");
        assert_eq!(aliases["child-b"], "subagent:2");
    }

    #[test]
    fn thread_handles_use_stable_thread_tail() {
        let record = agent(
            "019e97b6-854a-79b0-b212-ddff83db2ef3",
            Some("root"),
            "Pauli",
            Some("open"),
        );
        assert_eq!(thread_handle(&record), "thread:83db2ef3");
    }

    #[test]
    fn target_matching_accepts_name_handle_and_thread_id() {
        let record = agent(
            "019e97b6-854a-79b0-b212-ddff83db2ef3",
            Some("root"),
            "Pauli",
            Some("open"),
        );
        assert!(target_matches_agent(&record, "pauli"));
        assert!(target_matches_agent(&record, "83db"));
        assert!(target_matches_agent(&record, "thread:83db2ef3"));
    }

    #[test]
    fn friendly_aliases_disambiguate_repeated_names() {
        let directory = Directory {
            current_family: vec![
                agent("root", None, "Root", None),
                agent("child-a", Some("root"), "Dirac", Some("open")),
                agent("child-b", Some("root"), "Dirac", Some("open")),
                agent("child-c", Some("root"), "Noether", Some("open")),
            ],
        };

        let name_aliases = team_name_aliases(&directory.current_family);
        assert_eq!(name_aliases["child-a"], "Dirac#1");
        assert_eq!(name_aliases["child-b"], "Dirac#2");
        assert_eq!(name_aliases["child-c"], "Noether");

        assert_eq!(
            resolve_target(&directory, "Dirac#2").unwrap().thread_id,
            "child-b"
        );
        assert_eq!(
            resolve_target(&directory, "Noether").unwrap().thread_id,
            "child-c"
        );
        assert!(resolve_target(&directory, "Dirac").is_err());
    }

    #[test]
    fn friendly_aliases_accept_role_and_name_role() {
        let directory = Directory {
            current_family: vec![
                agent("root", None, "Root", None),
                agent_with_role("child-a", Some("root"), "Dirac", "reviewer", Some("open")),
                agent_with_role("child-b", Some("root"), "Dirac", "tester", Some("open")),
            ],
        };

        assert_eq!(
            resolve_target(&directory, "role:reviewer")
                .unwrap()
                .thread_id,
            "child-a"
        );
        assert_eq!(
            resolve_target(&directory, "Dirac:tester")
                .unwrap()
                .thread_id,
            "child-b"
        );
    }

    #[test]
    fn formats_thread_timestamps_as_utc() {
        assert_eq!(format_epoch_utc(1780697400), "2026-06-05T22:10:00Z");
    }
}
