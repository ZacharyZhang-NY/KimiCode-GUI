#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod llm;
mod oauth;
mod session;
mod tools;

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use tauri::{Emitter, Manager};
// 

pub use oauth::{OAuthToken, load_token, save_token, delete_token, is_logged_in};
pub use session::{Message, Session, SessionManager};

#[derive(Serialize)]
struct AppInfo {
    version: String,
    platform: String,
    arch: String,
}

#[derive(Serialize)]
struct AppPaths {
    config: String,
    mcp: String,
    gui: String,
    work_dir: String,
    share_dir: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
struct GuiSettings {
    work_dir: Option<String>,
    recent_work_dirs: Vec<String>,
    config_file: Option<String>,
    mcp_config_files: Vec<String>,
    skills_dir: Option<String>,
    model: Option<String>,
    thinking: Option<bool>,
    yolo: Option<bool>,
    pinned_sessions: Vec<String>,
    pinned_cowork_tasks: Vec<String>,
}

#[derive(Clone, Serialize)]
struct GuiSettingsPayload {
    path: String,
    settings: GuiSettings,
}

#[derive(Clone, Serialize)]
struct SkillInfo {
    name: String,
    description: Option<String>,
    path: String,
    root: String,
}

#[derive(Clone, Serialize)]
struct SkillsPayload {
    roots: Vec<String>,
    skills: Vec<SkillInfo>,
}

#[derive(Clone, Serialize)]
struct SessionInfo {
    id: String,
    title: String,
    updated_at: f64,
    work_dir: String,
}

#[derive(Clone, Serialize)]
struct AuthStatus {
    is_logged_in: bool,
    user: Option<String>,
    mode: String, // "oauth" | "api_key" | "none"
}

#[derive(Clone, Serialize)]
struct AgentBrowserStatus {
    available: bool,
    command: Option<String>,
    detail: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoworkHistoryStep {
    title: String,
    description: String,
    #[serde(default)]
    log: String,
    #[serde(default)]
    status: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CoworkHistoryEntry {
    id: String,
    prompt: String,
    status: String,
    folder: Option<String>,
    created_at: i64,
    updated_at: i64,
    #[serde(default)]
    steps: Vec<CoworkHistoryStep>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AuthConfig {
    pub mode: String, // "oauth" | "api_key"
    pub api_key: Option<String>,
    pub api_base: Option<String>,
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            mode: "oauth".to_string(),
            api_key: None,
            api_base: None,
        }
    }
}

fn auth_config_path() -> PathBuf {
    kimi_share_dir().join("gui_auth.json")
}

fn load_auth_config() -> AuthConfig {
    let path = auth_config_path();
    if let Ok(content) = fs::read_to_string(&path) {
        if let Ok(config) = serde_json::from_str::<AuthConfig>(&content) {
            return config;
        }
    }
    AuthConfig::default()
}

fn save_auth_config(config: &AuthConfig) -> Result<(), String> {
    let path = auth_config_path();
    let json = serde_json::to_string_pretty(config)
        .map_err(|e| format!("Failed to serialize auth config: {}", e))?;
    fs::write(&path, json)
        .map_err(|e| format!("Failed to write auth config: {}", e))?;
    Ok(())
}

#[tauri::command]
fn auth_get_config() -> AuthConfig {
    load_auth_config()
}

#[tauri::command]
fn auth_set_config(config: AuthConfig) -> Result<(), String> {
    save_auth_config(&config)
}

#[tauri::command]
fn auth_set_api_key(api_key: String, api_base: Option<String>) -> Result<(), String> {
    let config = AuthConfig {
        mode: "api_key".to_string(),
        api_key: Some(api_key),
        api_base: api_base.filter(|b| !b.is_empty()),
    };
    save_auth_config(&config)
}

fn command_exists(cmd: &str) -> bool {
    #[cfg(windows)]
    let mut check = {
        let mut c = Command::new("where");
        c.arg(cmd);
        c
    };

    #[cfg(not(windows))]
    let mut check = {
        let mut c = Command::new("which");
        c.arg(cmd);
        c
    };

    check
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

fn shell_escape_path(path: &Path) -> String {
    let value = path.to_string_lossy();
    #[cfg(windows)]
    {
        format!("\"{}\"", value.replace('\"', "\"\""))
    }
    #[cfg(not(windows))]
    {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

fn shell_command_with_agent_browser_home(binary: &Path, home: &Path) -> String {
    #[cfg(windows)]
    {
        let home_value = home.to_string_lossy().replace('\"', "\"\"");
        return format!(
            "set \"AGENT_BROWSER_HOME={home_value}\" && {}",
            shell_escape_path(binary)
        );
    }

    #[cfg(not(windows))]
    {
        format!(
            "AGENT_BROWSER_HOME={} {}",
            shell_escape_path(home),
            shell_escape_path(binary)
        )
    }
}

fn embedded_agent_browser_path(app: &tauri::AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    let target_key = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let binary_name = if cfg!(windows) {
        "agent-browser.exe"
    } else {
        "agent-browser"
    };

    let candidates = [
        resource_dir
            .join("agent-browser")
            .join(&target_key)
            .join(binary_name),
        resource_dir.join("agent-browser").join(binary_name),
    ];

    candidates.into_iter().find(|path| path.is_file())
}

fn embedded_agent_browser_home(app: &tauri::AppHandle) -> Option<PathBuf> {
    let resource_dir = app.path().resource_dir().ok()?;
    let target_key = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);

    let candidates = [
        resource_dir
            .join("agent-browser")
            .join(&target_key)
            .join("runtime")
            .join("node_modules")
            .join("agent-browser"),
        resource_dir
            .join("agent-browser")
            .join("runtime")
            .join("node_modules")
            .join("agent-browser"),
    ];

    candidates.into_iter().find(|path| {
        path.is_dir() && path.join("dist").join("daemon.js").is_file()
    })
}

#[tauri::command]
fn agent_browser_status(app: tauri::AppHandle) -> AgentBrowserStatus {
    let mut bundled_detail: Option<String> = None;

    if let Some(path) = embedded_agent_browser_path(&app) {
        if let Some(home) = embedded_agent_browser_home(&app) {
            return AgentBrowserStatus {
                available: true,
                command: Some(shell_command_with_agent_browser_home(&path, &home)),
                detail: format!(
                    "Bundled agent-browser found at {} with runtime {}",
                    path.to_string_lossy(),
                    home.to_string_lossy()
                ),
            };
        }
        bundled_detail = Some(format!(
            "Bundled agent-browser binary found at {}, but runtime is missing.",
            path.to_string_lossy()
        ));
    }

    if command_exists("agent-browser") {
        return AgentBrowserStatus {
            available: true,
            command: Some("agent-browser".to_string()),
            detail: "agent-browser is available in PATH.".to_string(),
        };
    }

    if command_exists("npx") {
        return AgentBrowserStatus {
            available: true,
            command: Some("npx --yes agent-browser".to_string()),
            detail: "agent-browser not found in PATH; npx fallback is available.".to_string(),
        };
    }

    AgentBrowserStatus {
        available: false,
        command: None,
        detail: bundled_detail.unwrap_or_else(|| {
            "Neither bundled agent-browser runtime, agent-browser in PATH, nor npx fallback is available.".to_string()
        }),
    }
}

fn agent_browser_policy(app: &tauri::AppHandle) -> String {
    let status = agent_browser_status(app.clone());
    if status.available {
        let command = status.command.unwrap_or_else(|| "agent-browser".to_string());
        format!(
            "Internet access policy:\n- For any request that needs internet/web pages, you MUST use the Shell tool with agent-browser.\n- Command prefix: {command}\n- Default flow: open <url> -> snapshot -i -> interact using @eN refs -> re-snapshot after navigation.\n- Do NOT use SearchWeb or FetchURL."
        )
    } else {
        format!(
            "Internet access policy:\n- For any request that needs internet/web pages, use Shell tool with agent-browser only.\n- agent-browser is currently unavailable in this environment ({detail}).\n- Report this limitation clearly and stop before attempting web access.\n- Do NOT use SearchWeb or FetchURL.",
            detail = status.detail
        )
    }
}

#[tauri::command]
fn auth_clear() -> Result<(), String> {
    // Clear OAuth token
    let _ = oauth::delete_token();
    // Clear API key config
    let path = auth_config_path();
    if path.exists() {
        let _ = fs::remove_file(&path);
    }
    Ok(())
}

struct AppState {
    sessions: Mutex<HashMap<u64, SessionHandle>>,
    next_id: AtomicU64,
    session_manager: Mutex<SessionManager>,
    approvals: Mutex<HashMap<String, tokio::sync::oneshot::Sender<bool>>>,
}

struct SessionHandle {
    cancel_tx: tokio::sync::oneshot::Sender<()>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            sessions: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(1),
            session_manager: Mutex::new(SessionManager::new()),
            approvals: Mutex::new(HashMap::new()),
        }
    }
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn kimi_share_dir() -> PathBuf {
    home_dir().join(".kimicodegui")
}

fn cowork_history_path() -> PathBuf {
    kimi_share_dir().join("cowork").join("history.json")
}

fn legacy_kimi_share_dir() -> PathBuf {
    home_dir().join(".kimi")
}

fn copy_file_if_missing(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.exists() || destination.exists() {
        return Ok(());
    }
    ensure_parent(destination)?;
    fs::copy(source, destination).map_err(|error| {
        format!(
            "Failed to migrate file from {source:?} to {destination:?}: {error}"
        )
    })?;
    Ok(())
}

fn copy_dir_if_missing(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Ok(());
    }

    if !destination.exists() {
        fs::create_dir_all(destination).map_err(|error| {
            format!("Failed to create migration directory {destination:?}: {error}")
        })?;
    }

    let entries = fs::read_dir(source)
        .map_err(|error| format!("Failed to read migration directory {source:?}: {error}"))?;

    for entry in entries {
        let entry =
            entry.map_err(|error| format!("Failed to read migration entry in {source:?}: {error}"))?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());
        let file_type = entry
            .file_type()
            .map_err(|error| format!("Failed to inspect {source_path:?}: {error}"))?;

        if file_type.is_dir() {
            copy_dir_if_missing(&source_path, &destination_path)?;
        } else if file_type.is_file() {
            copy_file_if_missing(&source_path, &destination_path)?;
        }
    }

    Ok(())
}

fn migrate_legacy_kimi_share_dir() -> Result<(), String> {
    let legacy = legacy_kimi_share_dir();
    let current = kimi_share_dir();
    if !legacy.exists() || legacy == current {
        return Ok(());
    }

    fs::create_dir_all(&current)
        .map_err(|error| format!("Failed to create {current:?}: {error}"))?;

    for file_name in [
        "config.toml",
        "mcp.json",
        "gui.json",
        "kimi.json",
        "gui_auth.json",
    ] {
        copy_file_if_missing(&legacy.join(file_name), &current.join(file_name))?;
    }

    for dir_name in ["credentials", "sessions", "gui_sessions"] {
        copy_dir_if_missing(&legacy.join(dir_name), &current.join(dir_name))?;
    }

    Ok(())
}

fn default_config_path() -> PathBuf {
    kimi_share_dir().join("config.toml")
}

fn default_mcp_path() -> PathBuf {
    kimi_share_dir().join("mcp.json")
}

fn default_gui_path() -> PathBuf {
    kimi_share_dir().join("gui.json")
}

fn metadata_path() -> PathBuf {
    kimi_share_dir().join("kimi.json")
}

fn ensure_parent(path: &Path) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("Failed to create directory {parent:?}: {error}"))?;
    }
    Ok(())
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|error| format!("Failed to read {path:?}: {error}"))
}

fn write_text(path: &Path, content: &str) -> Result<(), String> {
    ensure_parent(path)?;
    fs::write(path, content).map_err(|error| format!("Failed to write {path:?}: {error}"))
}

fn default_config_data() -> serde_json::Value {
    serde_json::json!({
        "default_model": "",
        "default_thinking": false,
        "models": {},
        "providers": {},
        "loop_control": {
            "max_steps_per_turn": 100,
            "max_retries_per_step": 3,
            "max_ralph_iterations": 0,
            "reserved_context_size": 50000
        },
        "services": {},
        "mcp": {
            "client": {
                "tool_call_timeout_ms": 60000
            }
        }
    })
}

fn strip_nulls(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(map) => {
            let keys: Vec<String> = map
                .iter()
                .filter_map(|(key, value)| value.is_null().then(|| key.clone()))
                .collect();
            for key in keys {
                map.remove(&key);
            }
            for value in map.values_mut() {
                strip_nulls(value);
            }
        }
        serde_json::Value::Array(list) => {
            list.retain(|value| !value.is_null());
            for value in list.iter_mut() {
                strip_nulls(value);
            }
        }
        _ => {}
    }
}

fn parse_config_content(path: &Path, raw: &str) -> Result<serde_json::Value, String> {
    if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        serde_json::from_str(raw)
            .map_err(|error| format!("Invalid JSON in {path:?}: {error}"))
    } else {
        let value: toml::Value =
            toml::from_str(raw).map_err(|error| format!("Invalid TOML in {path:?}: {error}"))?;
        serde_json::to_value(value)
            .map_err(|error| format!("Failed to convert TOML to JSON: {error}"))
    }
}

fn encode_config_content(path: &Path, data: &serde_json::Value) -> Result<String, String> {
    if path.extension().and_then(|ext| ext.to_str()) == Some("json") {
        serde_json::to_string_pretty(data)
            .map_err(|error| format!("Failed to encode JSON: {error}"))
    } else {
        toml::to_string(data).map_err(|error| format!("Failed to encode TOML: {error}"))
    }
}

fn normalize_legacy_storage_path(path: &str) -> String {
    let unix = path.replace("/.kimi/", "/.kimicodegui/");
    let unix = if unix.ends_with("/.kimi") {
        format!(
            "{}/.kimicodegui",
            unix.trim_end_matches("/.kimi")
        )
    } else {
        unix
    };

    let windows = unix.replace("\\.kimi\\", "\\.kimicodegui\\");
    if windows.ends_with("\\.kimi") {
        format!(
            "{}\\.kimicodegui",
            windows.trim_end_matches("\\.kimi")
        )
    } else {
        windows
    }
}

fn normalize_gui_settings_paths(mut settings: GuiSettings) -> GuiSettings {
    settings.config_file = settings
        .config_file
        .map(|path| normalize_legacy_storage_path(&path));
    settings.skills_dir = settings
        .skills_dir
        .map(|path| normalize_legacy_storage_path(&path));
    settings.mcp_config_files = settings
        .mcp_config_files
        .into_iter()
        .map(|path| normalize_legacy_storage_path(&path))
        .collect();
    let mut seen = std::collections::HashSet::new();
    settings.recent_work_dirs = settings
        .recent_work_dirs
        .into_iter()
        .map(|path| normalize_legacy_storage_path(&path))
        .filter(|path| !path.trim().is_empty())
        .filter(|path| seen.insert(path.clone()))
        .take(5)
        .collect();
    settings
}

fn find_repo_root() -> Option<PathBuf> {
    let mut current = std::env::current_dir().ok()?;
    loop {
        if current.join("pyproject.toml").is_file() && current.join("src/kimi_cli").is_dir() {
            return Some(current);
        }
        if !current.pop() {
            break;
        }
    }
    None
}

fn skills_root_candidates(work_dir: &Path) -> Vec<PathBuf> {
    vec![work_dir.join(".kimicodegui/skills")]
}

fn parse_skill_frontmatter(contents: &str) -> (Option<String>, Option<String>) {
    let mut lines = contents.lines();
    if lines.next().map(str::trim) != Some("---") {
        return (None, None);
    }

    let mut name = None;
    let mut description = None;

    for line in lines {
        let trimmed = line.trim();
        if trimmed == "---" {
            break;
        }
        if let Some((key, value)) = trimmed.split_once(':') {
            let value = value.trim().trim_matches('"').trim_matches('\'');
            match key.trim() {
                "name" => {
                    if !value.is_empty() {
                        name = Some(value.to_string());
                    }
                }
                "description" => {
                    if !value.is_empty() {
                        description = Some(value.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    (name, description)
}

fn truncate_with_ellipsis(input: &str, max_chars: usize) -> String {
    let total = input.chars().count();
    if total <= max_chars {
        return input.to_string();
    }
    if max_chars <= 3 {
        return input.chars().take(max_chars).collect();
    }
    let prefix: String = input.chars().take(max_chars - 3).collect();
    format!("{prefix}...")
}

fn collect_skills(root: &Path) -> Vec<SkillInfo> {
    let mut skills = Vec::new();
    let entries = match fs::read_dir(root) {
        Ok(entries) => entries,
        Err(_) => return skills,
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_file = path.join("SKILL.md");
        if !skill_file.is_file() {
            continue;
        }
        let contents = fs::read_to_string(&skill_file).unwrap_or_default();
        let (name, description) = parse_skill_frontmatter(&contents);
        let fallback_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("skill")
            .to_string();
        skills.push(SkillInfo {
            name: name.unwrap_or(fallback_name),
            description,
            path: skill_file.to_string_lossy().to_string(),
            root: root.to_string_lossy().to_string(),
        });
    }

    skills
}

fn load_sessions(work_dir: &str) -> Result<Vec<SessionInfo>, String> {
    let meta_path = metadata_path();
    if !meta_path.exists() {
        return Ok(Vec::new());
    }

    let raw = read_text(&meta_path)?;
    let data: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| format!("Failed to parse metadata: {e}"))?;

    let empty_vec = Vec::new();
    let work_dirs = data.get("work_dirs").and_then(|v| v.as_array()).unwrap_or(&empty_vec);
    
    for wd in work_dirs {
        let path = wd.get("path").and_then(|v| v.as_str()).unwrap_or("");
        if path == work_dir {
            let kaos = wd.get("kaos").and_then(|v| v.as_str()).unwrap_or("local");
            let sessions_dir = get_session_dir(path, kaos)?;
            
            let mut sessions = Vec::new();
            if let Ok(entries) = fs::read_dir(&sessions_dir) {
                for entry in entries.flatten() {
                    let session_path = entry.path();
                    if !session_path.is_dir() {
                        continue;
                    }
                    
                    let session_id = session_path.file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("")
                        .to_string();
                    
                    let context_file = session_path.join("context.jsonl");
                    let wire_file = session_path.join("wire.jsonl");
                    
                    if !context_file.exists() {
                        continue;
                    }
                    
                    let updated_at = context_file.metadata()
                        .and_then(|m| m.modified())
                        .ok()
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs_f64())
                        .unwrap_or(0.0);
                    
                    let title = extract_session_title(&wire_file).unwrap_or_else(|| {
                        format!("Session {}", &session_id[..8.min(session_id.len())])
                    });
                    
                    sessions.push(SessionInfo {
                        id: session_id,
                        title,
                        updated_at,
                        work_dir: path.to_string(),
                    });
                }
            }
            
            sessions.sort_by(|a, b| b.updated_at.partial_cmp(&a.updated_at).unwrap());
            return Ok(sessions);
        }
    }
    
    Ok(Vec::new())
}

fn get_session_dir(work_dir: &str, kaos: &str) -> Result<PathBuf, String> {
    use std::hash::{Hash, Hasher};
    use std::collections::hash_map::DefaultHasher;
    
    let mut hasher = DefaultHasher::new();
    work_dir.hash(&mut hasher);
    let hash = format!("{:016x}", hasher.finish());
    
    let dir_name = if kaos == "local" {
        hash
    } else {
        format!("{}_{}", kaos, hash)
    };
    
    let session_dir = kimi_share_dir().join("sessions").join(dir_name);
    Ok(session_dir)
}

fn extract_session_title(wire_file: &Path) -> Option<String> {
    if !wire_file.exists() {
        return None;
    }
    
    let content = fs::read_to_string(wire_file).ok()?;
    
    for line in content.lines().take(50) {
        if let Ok(record) = serde_json::from_str::<serde_json::Value>(line) {
            if record.get("type").and_then(|v| v.as_str()) == Some("turn_begin") {
                if let Some(input) = record.get("user_input").and_then(|v| v.as_str()) {
                    let title = truncate_with_ellipsis(input, 50);
                    return Some(title);
                }
            }
        }
    }
    
    None
}

fn load_cowork_history_entries() -> Result<Vec<CoworkHistoryEntry>, String> {
    let path = cowork_history_path();
    if !path.exists() {
        return Ok(Vec::new());
    }

    let raw = read_text(&path)?;
    let mut entries: Vec<CoworkHistoryEntry> =
        serde_json::from_str(&raw).map_err(|error| format!("Invalid cowork history JSON: {error}"))?;
    entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(entries)
}

fn save_cowork_history_entries(entries: &[CoworkHistoryEntry]) -> Result<(), String> {
    let path = cowork_history_path();
    let raw = serde_json::to_string_pretty(entries)
        .map_err(|error| format!("Failed to encode cowork history JSON: {error}"))?;
    write_text(&path, &raw)
}

#[tauri::command]
fn cowork_history_load() -> Result<Vec<CoworkHistoryEntry>, String> {
    load_cowork_history_entries()
}

#[tauri::command]
fn cowork_history_upsert(entry: CoworkHistoryEntry) -> Result<(), String> {
    if entry.id.trim().is_empty() {
        return Err("History entry id cannot be empty".to_string());
    }

    let mut entries = load_cowork_history_entries()?;
    if let Some(index) = entries.iter().position(|item| item.id == entry.id) {
        entries[index] = entry;
    } else {
        entries.push(entry);
    }

    entries.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    entries.dedup_by(|a, b| a.id == b.id);
    if entries.len() > 500 {
        entries.truncate(500);
    }

    save_cowork_history_entries(&entries)
}

#[tauri::command]
fn cowork_history_delete(entry_id: String) -> Result<(), String> {
    let id = entry_id.trim();
    if id.is_empty() {
        return Err("History entry id cannot be empty".to_string());
    }

    let mut entries = load_cowork_history_entries()?;
    entries.retain(|item| item.id != id);
    save_cowork_history_entries(&entries)
}

#[tauri::command]
fn app_info() -> AppInfo {
    AppInfo {
        version: env!("CARGO_PKG_VERSION").to_string(),
        platform: match std::env::consts::OS {
            "macos" => "macOS",
            "windows" => "Windows",
            "linux" => "Linux",
            other => other,
        }
        .to_string(),
        arch: std::env::consts::ARCH.to_string(),
    }
}

#[tauri::command]
fn app_paths() -> AppPaths {
    let work_dir = find_repo_root().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    });

    AppPaths {
        config: default_config_path().to_string_lossy().to_string(),
        mcp: default_mcp_path().to_string_lossy().to_string(),
        gui: default_gui_path().to_string_lossy().to_string(),
        work_dir: work_dir.to_string_lossy().to_string(),
        share_dir: kimi_share_dir().to_string_lossy().to_string(),
    }
}

#[tauri::command]
fn config_load(path: Option<String>) -> Result<session::ConfigPayload, String> {
    let path = path
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);

    if !path.exists() {
        let data = default_config_data();
        let mut clean = data.clone();
        strip_nulls(&mut clean);
        let raw = encode_config_content(&path, &clean)?;
        write_text(&path, &raw)?;
    }

    let raw = read_text(&path)?;
    let data = parse_config_content(&path, &raw)?;

    Ok(session::ConfigPayload {
        path: path.to_string_lossy().to_string(),
        raw,
        data,
    })
}

#[tauri::command]
fn config_save(path: Option<String>, data: serde_json::Value) -> Result<(), String> {
    let path = path
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let mut clean = data.clone();
    strip_nulls(&mut clean);
    let raw = encode_config_content(&path, &clean)?;
    write_text(&path, &raw)?;
    Ok(())
}

#[tauri::command]
fn config_save_raw(path: Option<String>, raw: String) -> Result<(), String> {
    let path = path
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    parse_config_content(&path, &raw)?;
    write_text(&path, &raw)?;
    Ok(())
}

#[tauri::command]
fn mcp_load(path: Option<String>) -> Result<session::McpPayload, String> {
    let path = path.map(PathBuf::from).unwrap_or_else(default_mcp_path);
    if !path.exists() {
        let raw = serde_json::json!({ "mcpServers": {} });
        let content =
            serde_json::to_string_pretty(&raw).map_err(|error| error.to_string())?;
        write_text(&path, &content)?;
    }
    let raw = read_text(&path)?;
    let data: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("Invalid MCP JSON: {error}"))?;

    Ok(session::McpPayload {
        path: path.to_string_lossy().to_string(),
        raw,
        data,
    })
}

#[tauri::command]
fn mcp_save(path: Option<String>, data: serde_json::Value) -> Result<(), String> {
    let path = path.map(PathBuf::from).unwrap_or_else(default_mcp_path);
    let raw = serde_json::to_string_pretty(&data).map_err(|error| error.to_string())?;
    write_text(&path, &raw)?;
    Ok(())
}

#[tauri::command]
fn mcp_save_raw(path: Option<String>, raw: String) -> Result<(), String> {
    let path = path.map(PathBuf::from).unwrap_or_else(default_mcp_path);
    let _: serde_json::Value =
        serde_json::from_str(&raw).map_err(|error| format!("Invalid MCP JSON: {error}"))?;
    write_text(&path, &raw)?;
    Ok(())
}

#[tauri::command]
fn gui_settings_load(path: Option<String>) -> Result<GuiSettingsPayload, String> {
    let path = path.map(PathBuf::from).unwrap_or_else(default_gui_path);
    if !path.exists() {
        return Ok(GuiSettingsPayload {
            path: path.to_string_lossy().to_string(),
            settings: GuiSettings::default(),
        });
    }
    let raw = read_text(&path)?;
    let settings: GuiSettings =
        serde_json::from_str(&raw).map_err(|error| format!("Invalid GUI settings: {error}"))?;
    let settings = normalize_gui_settings_paths(settings);
    Ok(GuiSettingsPayload {
        path: path.to_string_lossy().to_string(),
        settings,
    })
}

#[tauri::command]
fn gui_settings_save(path: Option<String>, settings: GuiSettings) -> Result<(), String> {
    let path = path.map(PathBuf::from).unwrap_or_else(default_gui_path);
    let raw = serde_json::to_string_pretty(&settings).map_err(|error| error.to_string())?;
    write_text(&path, &raw)?;
    Ok(())
}

#[tauri::command]
fn skills_list(work_dir: Option<String>, skills_dir: Option<String>) -> Result<SkillsPayload, String> {
    let work_dir = work_dir.map(PathBuf::from);

    let mut roots = Vec::new();
    if let Some(skills_dir) = skills_dir {
        let root = PathBuf::from(skills_dir);
        if root.is_dir() {
            roots.push(root);
        }
    } else {
        let global_root = kimi_share_dir().join("skills");
        if global_root.is_dir() {
            roots.push(global_root);
        }
        if let Some(work_dir) = work_dir {
            for root in skills_root_candidates(&work_dir) {
                if root.is_dir() {
                    roots.push(root);
                }
            }
        }
    }

    let mut seen = HashMap::new();
    let mut skills = Vec::new();
    for root in &roots {
        for skill in collect_skills(root) {
            let key = skill.name.to_lowercase();
            if !seen.contains_key(&key) {
                seen.insert(key, true);
                skills.push(skill);
            }
        }
    }

    Ok(SkillsPayload {
        roots: roots
            .into_iter()
            .map(|root| root.to_string_lossy().to_string())
            .collect(),
        skills,
    })
}

#[tauri::command]
fn session_list(
    state: tauri::State<'_, AppState>,
    work_dir: Option<String>
) -> Result<Vec<SessionInfo>, String> {
    let mut sessions = Vec::new();
    
    // Load CLI sessions if work_dir is provided
    if let Some(ref wd) = work_dir {
        sessions = load_sessions(wd)?;
    }
    
    // Also load GUI sessions from SessionManager
    let mut manager = state.session_manager.lock()
        .map_err(|_| "Session manager poisoned".to_string())?;
    
    if let Ok(gui_sessions) = manager.load_all_sessions() {
        for session in &gui_sessions {
            let include = if let Some(ref wd) = work_dir {
                // Normalize paths for comparison
                let session_path = Path::new(&session.work_dir).canonicalize().ok().unwrap_or_else(|| Path::new(&session.work_dir).to_path_buf());
                let work_path = Path::new(wd).canonicalize().ok().unwrap_or_else(|| Path::new(wd).to_path_buf());
                session_path == work_path || session.work_dir == *wd
            } else {
                // If no work_dir filter, include all sessions
                true
            };
            
            if include {
                sessions.push(SessionInfo {
                    id: session.id.clone(),
                    title: session.title.clone(),
                    updated_at: session.updated_at as f64,
                    work_dir: session.work_dir.clone(),
                });
            }
        }
    }
    
    // Sort by updated_at descending
    sessions.sort_by(|a, b| b.updated_at.partial_cmp(&a.updated_at).unwrap());
    
    // Remove duplicates (same id)
    let mut seen = HashMap::new();
    let mut unique = Vec::new();
    for s in sessions {
        if !seen.contains_key(&s.id) {
            seen.insert(s.id.clone(), true);
            unique.push(s);
        }
    }
    
    Ok(unique)
}

#[tauri::command]
fn auth_check_status() -> Result<AuthStatus, String> {
    // Check OAuth
    let oauth_logged_in = oauth::is_logged_in();
    
    // Check API Key
    let config = load_auth_config();
    let api_key_valid = config.mode == "api_key" && config.api_key.as_ref().map(|k| !k.is_empty()).unwrap_or(false);
    
    let is_logged_in = oauth_logged_in || api_key_valid;
    let mode = if oauth_logged_in {
        "oauth"
    } else if api_key_valid {
        "api_key"
    } else {
        "none"
    };
    
    Ok(AuthStatus {
        is_logged_in,
        user: if is_logged_in { Some("User".to_string()) } else { None },
        mode: mode.to_string(),
    })
}

#[tauri::command]
fn session_messages(
    state: tauri::State<'_, AppState>,
    work_dir: String, 
    session_id: String
) -> Result<Vec<Message>, String> {
    // First try GUI sessions from memory (most common case)
    {
        let manager = state.session_manager.lock()
            .map_err(|_| "Session manager poisoned".to_string())?;
        
        if let Some(session) = manager.sessions.get(&session_id) {
            return Ok(session.messages.clone());
        }
    }
    
    // Try loading from disk
    {
        let mut manager = state.session_manager.lock()
            .map_err(|_| "Session manager poisoned".to_string())?;
        
        match manager.load_all_sessions() {
            Ok(sessions) => {
                for session in sessions {
                    if session.id == session_id {
                        return Ok(session.messages);
                    }
                }
            }
            Err(_) => {}
        }
    }
    
    // Finally try CLI sessions (from wire files)
    {
        let manager = state.session_manager.lock()
            .map_err(|_| "Session manager poisoned".to_string())?;
        
        match manager.load_messages(&work_dir, &session_id) {
            Ok(messages) => {
                if !messages.is_empty() {
                    return Ok(messages);
                }
            }
            Err(_) => {}
        }
    }
    
    Ok(Vec::new())
}

#[tauri::command]
fn session_save_message(
    state: tauri::State<'_, AppState>,
    session_id: String,
    role: String,
    content: String,
) -> Result<(), String> {
    use crate::session::Message as SessionMessage;
    
    let mut manager = state.session_manager.lock()
        .map_err(|_| "Session manager poisoned".to_string())?;
    
    let message = SessionMessage {
        role: role.clone(),
        content: content.clone(),
        timestamp: chrono::Utc::now().timestamp(),
        tool_calls: None,
    };
    
    // Save to file and add to memory
    match manager.save_message(&session_id, &message) {
        Ok(_) => {}
        Err(_) => {}
    }
    
    match manager.add_message(&session_id, message) {
        Ok(_) => {}
        Err(_) => {}
    }
    Ok(())
}

#[tauri::command]
fn session_delete(
    state: tauri::State<'_, AppState>,
    work_dir: String,
    session_id: String,
) -> Result<(), String> {
    let mut manager = state
        .session_manager
        .lock()
        .map_err(|_| "Session manager poisoned".to_string())?;
    manager.delete_session(&work_dir, &session_id)?;
    Ok(())
}

#[derive(Clone, Serialize)]
pub struct CoworkStreamEvent {
    pub event: String,
    pub data: serde_json::Value,
}

#[tauri::command]
async fn cowork_stream(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
    prompt: String,
    folder: String,
    model: String,
    system_prompt: String,
) -> Result<(), String> {

    
    // Load auth config
    let auth_config = load_auth_config();
    
    // Get cancel channel
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    {
        let mut sessions = state.sessions.lock()
            .map_err(|_| "Session store poisoned".to_string())?;
        let stream_id = state.next_id.fetch_add(1, Ordering::Relaxed);
        sessions.insert(stream_id, SessionHandle { cancel_tx });
    }
    
    // Use YOLO mode for cowork (auto-approve most tools)
    let auto_approve = true;
    
    // Wrap the window to emit to cowork://event instead of chat://event
    let window_clone = window.clone();
    
    // Call the existing stream_chat but intercept events
    // For simplicity, we'll emit a step event at the start
    let _ = window.emit(
        "cowork://event",
        CoworkStreamEvent {
            event: "step".to_string(),
            data: serde_json::json!({
                "title": "Starting task",
                "description": prompt.clone(),
            }),
        },
    );
    
    // Use the same underlying LLM call but with cowork-specific system prompt
    let config_path = Some(app_paths().config);
    
    let policy = agent_browser_policy(&window.app_handle());
    let combined_prompt = if system_prompt.trim().is_empty() {
        policy
    } else {
        format!("{system_prompt}\n\n{policy}")
    };
    let extra_system_prompt = Some(combined_prompt);

    let result = llm::stream_chat(
        window_clone.clone(),
        state.clone(),
        session_id.clone(),
        prompt,
        model,
        folder,
        config_path,
        "cowork://event",
        extra_system_prompt,
        auto_approve,
        auth_config,
        cancel_rx,
    ).await;
    
    // Emit completion event
    match &result {
        Ok(_) => {
            let _ = window.emit(
                "cowork://event",
                CoworkStreamEvent {
                    event: "done".to_string(),
                    data: serde_json::json!({}),
                },
            );
        }
        Err(e) => {
            let _ = window.emit(
                "cowork://event",
                CoworkStreamEvent {
                    event: "error".to_string(),
                    data: serde_json::json!({
                        "message": e.to_string(),
                    }),
                },
            );
        }
    }
    
    result
}

#[tauri::command]
async fn chat_stream(
    window: tauri::Window,
    state: tauri::State<'_, AppState>,
    session_id: String,
    message: String,
    settings: Option<GuiSettings>,
) -> Result<(), String> {
    use crate::session::{Message as SessionMessage};
    
    let settings = settings.unwrap_or_default();
    
    let model = settings.model
        .filter(|m| !m.is_empty())
        .unwrap_or_else(|| "kimi-k2.5".to_string());
    
    let work_dir = settings
        .work_dir
        .filter(|path| !path.trim().is_empty())
        .unwrap_or_else(|| ".".to_string());

    let config_path = settings
        .config_file
        .filter(|path| !path.is_empty())
        .or_else(|| Some(app_paths().config));

    let auto_approve = settings.yolo.unwrap_or(false);
    
    // Load auth config
    let auth_config = load_auth_config();
    
    let title = truncate_with_ellipsis(&message, 50);
    
    // Create or get session and save user message
    {
        let mut manager = state.session_manager.lock()
            .map_err(|_| "Session manager poisoned".to_string())?;
        
        // Get or create session
        let _session = manager.get_or_create_session(&session_id, &title, &work_dir);
        
        // Save user message
        let user_msg = SessionMessage {
            role: "user".to_string(),
            content: message.clone(),
            timestamp: chrono::Utc::now().timestamp(),
            tool_calls: None,
        };
        let _ = manager.save_message(&session_id, &user_msg);
        let _ = manager.add_message(&session_id, user_msg);
    }
    
    let (cancel_tx, cancel_rx) = tokio::sync::oneshot::channel();
    
    {
        let mut sessions = state.sessions.lock()
            .map_err(|_| "Session store poisoned".to_string())?;
        let stream_id = state.next_id.fetch_add(1, Ordering::Relaxed);
        sessions.insert(stream_id, SessionHandle { cancel_tx });
    }
    
    let window_clone = window.clone();
    let session_id_clone = session_id.clone();
    
    // Wrap the stream_chat to capture the response
    let result = llm::stream_chat(
        window_clone,
        state.clone(),
        session_id_clone,
        message,
        model,
        work_dir.clone(),
        config_path,
        "chat://event",
        Some(agent_browser_policy(&window.app_handle())),
        auto_approve,
        auth_config,
        cancel_rx,
    ).await;
    
    // Note: We can't easily capture the content from stream_chat since it emits to window.
    // For now, sessions will be tracked but full message persistence requires 
    // either a callback mechanism or frontend sending back the complete response.
    
    // Update session timestamp
    {
        let mut manager = state.session_manager.lock()
            .map_err(|_| "Session manager poisoned".to_string())?;
        let now = chrono::Utc::now().timestamp();
        if let Some(session) = manager.sessions.get_mut(&session_id) {
            session.updated_at = now;
            let session_clone = session.clone();
            let _ = manager.save_session(&session_clone);
        }
    }
    
    result
}

#[tauri::command]
fn tool_approval_respond(
    state: tauri::State<'_, AppState>,
    request_id: String,
    approved: bool,
) -> Result<(), String> {
    let mut approvals = state
        .approvals
        .lock()
        .map_err(|_| "Approval store poisoned".to_string())?;
    if let Some(tx) = approvals.remove(&request_id) {
        let _ = tx.send(approved);
        Ok(())
    } else {
        Err("Approval request not found".to_string())
    }
}

#[tauri::command]
fn cancel_chat(state: tauri::State<'_, AppState>) -> Result<(), String> {
    let mut sessions = state.sessions.lock()
        .map_err(|_| "Session store poisoned".to_string())?;
    
    for (_, handle) in sessions.drain() {
        let _ = handle.cancel_tx.send(());
    }
    
    Ok(())
}

#[tauri::command]
fn list_files(work_dir: String, query: Option<String>) -> Result<Vec<String>, String> {
    let root = Path::new(&work_dir);
    if !root.exists() {
        return Ok(Vec::new());
    }
    
    let mut files = Vec::new();
    let query_lower = query.unwrap_or_default().to_lowercase();
    
    fn is_ignored(name: &str) -> bool {
        let ignored = [
            ".git", ".svn", ".hg", ".DS_Store",
            "node_modules", "target", "dist", "build",
            ".venv", "venv", "__pycache__", ".pytest_cache",
            ".idea", ".vscode", ".next", ".nuxt",
        ];
        ignored.iter().any(|&i| name == i || name.starts_with('.'))
    }
    
    fn walk_dir(path: &Path, root: &Path, files: &mut Vec<String>, query: &str, limit: usize) {
        if files.len() >= limit {
            return;
        }
        
        if let Ok(entries) = std::fs::read_dir(path) {
            for entry in entries.flatten() {
                if files.len() >= limit {
                    break;
                }
                
                let name = entry.file_name().to_string_lossy().to_string();
                if is_ignored(&name) {
                    continue;
                }
                
                let path = entry.path();
                let rel_path = path.strip_prefix(root).unwrap_or(&path);
                let rel_str = rel_path.to_string_lossy().to_string();
                
                if query.is_empty() || rel_str.to_lowercase().contains(query) {
                    files.push(rel_str);
                }
                
                if path.is_dir() {
                    walk_dir(&path, root, files, query, limit);
                }
            }
        }
    }
    
    walk_dir(root, root, &mut files, &query_lower, 50);
    files.sort();
    Ok(files)
}

#[tauri::command]
fn read_file(work_dir: String, file_path: String) -> Result<String, String> {
    let root = Path::new(&work_dir);
    let full_path = root.join(&file_path);
    
    // Security: ensure the path is within work_dir
    let canonical = full_path.canonicalize()
        .map_err(|e| format!("Failed to resolve path: {}", e))?;
    let canonical_root = root.canonicalize()
        .map_err(|e| format!("Failed to resolve work dir: {}", e))?;
    
    if !canonical.starts_with(&canonical_root) {
        return Err("Path is outside working directory".to_string());
    }
    
    // Limit file size to 100KB
    let metadata = std::fs::metadata(&canonical)
        .map_err(|e| format!("Failed to read file metadata: {}", e))?;
    
    if metadata.len() > 100_000 {
        return Err("File too large (max 100KB)".to_string());
    }
    
    std::fs::read_to_string(&canonical)
        .map_err(|e| format!("Failed to read file: {}", e))
}

#[tauri::command]
async fn pick_folder(app: tauri::AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;
    
    // Use blocking_pick_folder in async context (it runs on main thread)
    let folder = app.dialog().file().blocking_pick_folder();
    
    Ok(folder.map(|p| p.to_string()))
}

#[derive(Clone, Serialize)]
struct DirEntry {
    name: String,
    path: String,
    is_dir: bool,
    children: Option<Vec<DirEntry>>,
}

#[derive(Clone, Serialize)]
struct DirTree {
    path: String,
    name: String,
    entries: Vec<DirEntry>,
    git_status: Vec<GitStatusEntry>,
}

#[tauri::command]
fn list_dir_tree(path: String) -> Result<DirTree, String> {
    let root = Path::new(&path);
    if !root.exists() {
        return Err("Path does not exist".to_string());
    }
    
    if !root.is_dir() {
        return Err("Path is not a directory".to_string());
    }
    
    fn is_ignored(name: &str) -> bool {
        let ignored = [
            ".git", ".svn", ".hg", ".DS_Store",
            "node_modules", "target", "dist", "build",
            ".venv", "venv", "__pycache__", ".pytest_cache",
            ".idea", ".vscode", ".next", ".nuxt",
            "Cargo.lock", "package-lock.json", "yarn.lock", "pnpm-lock.yaml",
        ];
        ignored.iter().any(|&i| name == i || name.starts_with('.'))
    }
    
    fn read_dir_recursive(path: &Path, root: &Path, depth: usize) -> Result<Vec<DirEntry>, String> {
        if depth > 10 {
            return Ok(Vec::new()); // Limit depth
        }
        
        let mut entries = Vec::new();
        
        let dir_entries = match std::fs::read_dir(path) {
            Ok(entries) => entries,
            Err(_) => return Ok(Vec::new()), // Skip directories we can't read
        };
        
        for entry in dir_entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            
            if is_ignored(&name) {
                continue;
            }
            
            let full_path = entry.path();
            let path_str = full_path.to_string_lossy().to_string();
            let is_dir = full_path.is_dir();
            
            let children = if is_dir && depth < 2 {
                // Only load children for first 2 levels initially
                Some(read_dir_recursive(&full_path, root, depth + 1)?)
            } else if is_dir {
                Some(Vec::new()) // Empty vec to indicate it has children but not loaded yet
            } else {
                None
            };
            
            entries.push(DirEntry {
                name,
                path: path_str,
                is_dir,
                children,
            });
        }
        
        // Sort: directories first, then by name
        entries.sort_by(|a, b| {
            match (a.is_dir, b.is_dir) {
                (true, false) => std::cmp::Ordering::Less,
                (false, true) => std::cmp::Ordering::Greater,
                _ => a.name.cmp(&b.name),
            }
        });
        
        Ok(entries)
    }
    
    let entries = read_dir_recursive(root, root, 0)?;
    
    // Get git status for all files
    let git_status = get_git_status(root);
    
    Ok(DirTree {
        path: path.clone(),
        name: root.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("root")
            .to_string(),
        entries,
        git_status,
    })
}

#[derive(Clone, Serialize)]
struct GitStatusEntry {
    path: String,
    status: String, // "modified", "added", "deleted", "untracked", "staged"
}

fn get_git_status(root: &Path) -> Vec<GitStatusEntry> {
    let mut status_entries = Vec::new();
    
    // Check if it's a git repo
    let git_dir = root.join(".git");
    if !git_dir.exists() {
        return status_entries;
    }
    
    // Run git status --porcelain
    let output = std::process::Command::new("git")
        .args(["-C", root.to_str().unwrap_or("."), "status", "--porcelain"])
        .output();
    
    if let Ok(output) = output {
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.len() < 3 {
                continue;
            }
            let status_code = &line[0..2];
            let file_path = line[3..].to_string();
            
            let status = match status_code {
                "M " | "M" => "staged",
                " M" => "modified",
                "A " | "A" => "added",
                "D " | " D" => "deleted",
                "??" => "untracked",
                _ => "modified",
            };
            
            status_entries.push(GitStatusEntry {
                path: file_path,
                status: status.to_string(),
            });
        }
    }
    
    status_entries
}

#[tauri::command]
fn write_file(work_dir: String, file_path: String, content: String) -> Result<(), String> {
    let root = Path::new(&work_dir);
    let full_path = root.join(&file_path);
    
    // Security: ensure the path is within work_dir
    let canonical = full_path.canonicalize()
        .map_err(|e| format!("Failed to resolve path: {}", e))?;
    let canonical_root = root.canonicalize()
        .map_err(|e| format!("Failed to resolve work dir: {}", e))?;
    
    if !canonical.starts_with(&canonical_root) {
        return Err("Path is outside working directory".to_string());
    }
    
    std::fs::write(&canonical, content)
        .map_err(|e| format!("Failed to write file: {}", e))
}

fn main() {
    let _ = migrate_legacy_kimi_share_dir();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            app_info,
            app_paths,
            config_load,
            config_save,
            config_save_raw,
            mcp_load,
            mcp_save,
            mcp_save_raw,
            gui_settings_load,
            gui_settings_save,
            skills_list,
            session_list,
            auth_check_status,
            auth_get_config,
            auth_set_config,
            auth_set_api_key,
            auth_clear,
            agent_browser_status,
            cowork_history_load,
            cowork_history_upsert,
            cowork_history_delete,
            session_messages,
            session_save_message,
            session_delete,
            chat_stream,
            cowork_stream,
            cancel_chat,
            list_files,
            read_file,
            write_file,
            pick_folder,
            list_dir_tree,
            tool_approval_respond,
            // OAuth commands
            oauth::oauth_check_status,
            oauth::oauth_logout,
            oauth::oauth_start_login,
            oauth::oauth_open_browser,
            oauth::oauth_get_user,
            // LLM commands
            llm::llm_fetch_models,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
