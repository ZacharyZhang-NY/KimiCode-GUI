use reqwest::header::CONTENT_TYPE;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use tokio::process::Command;
use tokio::time::{timeout, Duration};

use crate::oauth::common_headers;
const MAX_LINES: usize = 1000;
const MAX_LINE_LENGTH: usize = 2000;
const MAX_BYTES: usize = 100_000;
const MAX_OUTPUT_CHARS: usize = 50_000;
const MAX_OUTPUT_LINE_LENGTH: usize = 2000;
const TRUNCATION_MARKER: &str = "[...truncated]";

#[derive(Clone, Debug)]
pub struct ToolOutput {
    pub ok: bool,
    pub summary: String,
    pub output: String,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    search_results: Vec<SearchResult>,
}

#[derive(Debug, Deserialize)]
struct SearchResult {
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    snippet: String,
    #[serde(default)]
    content: String,
    #[serde(default)]
    date: String,
}

#[derive(Clone, Debug)]
struct ServiceConfig {
    base_url: String,
    api_key: String,
    custom_headers: HashMap<String, String>,
}

fn home_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))
}

fn default_config_path() -> PathBuf {
    home_dir().join(".kimi").join("config.toml")
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

fn load_config_value(config_path: Option<&str>) -> Result<serde_json::Value, String> {
    let path = config_path
        .map(PathBuf::from)
        .unwrap_or_else(default_config_path);
    let raw = fs::read_to_string(&path)
        .map_err(|error| format!("Failed to read config {path:?}: {error}"))?;
    parse_config_content(&path, &raw)
}

fn parse_service_config(value: &serde_json::Value, key: &str) -> Option<ServiceConfig> {
    let services = value.get("services")?;
    let service = services.get(key)?;
    let base_url = service.get("base_url")?.as_str()?.to_string();
    let api_key = service.get("api_key")?.as_str()?.to_string();
    let custom_headers = service
        .get("custom_headers")
        .and_then(|v| v.as_object())
        .map(|map| {
            map.iter()
                .filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string())))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();
    Some(ServiceConfig {
        base_url,
        api_key,
        custom_headers,
    })
}

fn resolve_path(work_dir: &str, path: &str, must_exist: bool) -> Result<PathBuf, String> {
    if path.trim().is_empty() {
        return Err("Path cannot be empty".to_string());
    }

    let input_path = Path::new(path);
    let base = Path::new(work_dir);
    let target = if input_path.is_absolute() {
        input_path.to_path_buf()
    } else {
        base.join(input_path)
    };

    if must_exist {
        let canonical = target
            .canonicalize()
            .map_err(|e| format!("Failed to resolve path: {e}"))?;
        if !input_path.is_absolute() {
            let canonical_root = base
                .canonicalize()
                .map_err(|e| format!("Failed to resolve work dir: {e}"))?;
            if !canonical.starts_with(&canonical_root) {
                return Err("Path is outside working directory".to_string());
            }
        }
        return Ok(canonical);
    }

    if !input_path.is_absolute() {
        let canonical_root = base
            .canonicalize()
            .map_err(|e| format!("Failed to resolve work dir: {e}"))?;
        let parent = target
            .parent()
            .ok_or_else(|| "Invalid path".to_string())?;
        let canonical_parent = parent
            .canonicalize()
            .map_err(|e| format!("Failed to resolve path: {e}"))?;
        if !canonical_parent.starts_with(&canonical_root) {
            return Err("Path is outside working directory".to_string());
        }
    }

    Ok(target)
}

fn truncate_line(line: &str) -> (String, bool) {
    let line_len = line.chars().count();
    if line_len <= MAX_LINE_LENGTH {
        return (line.to_string(), false);
    }

    let marker = "...";
    let marker_len = marker.chars().count();
    let max_len = MAX_LINE_LENGTH.max(marker_len);
    let take_len = max_len.saturating_sub(marker_len);
    let prefix: String = line.chars().take(take_len).collect();
    let mut out = String::new();
    out.push_str(&prefix);
    out.push_str(marker);
    (out, true)
}

fn truncate_output(text: &str) -> (String, bool) {
    let mut output = String::new();
    let mut total_chars = 0usize;
    let mut truncated = false;

    for line in text.split_inclusive('\n') {
        if total_chars >= MAX_OUTPUT_CHARS {
            truncated = true;
            break;
        }

        let (line_body, line_break) = if let Some(stripped) = line.strip_suffix("\r\n") {
            (stripped, "\r\n")
        } else if let Some(stripped) = line.strip_suffix('\n') {
            (stripped, "\n")
        } else {
            (line, "")
        };

        let (line_text, line_truncated) =
            truncate_output_line(line_body, MAX_OUTPUT_LINE_LENGTH, line_break);
        if line_truncated {
            truncated = true;
        }

        let remaining = MAX_OUTPUT_CHARS.saturating_sub(total_chars);
        if remaining == 0 {
            truncated = true;
            break;
        }

        let line_chars: usize = line_text.chars().count();
        if line_chars > remaining {
            let partial: String = line_text.chars().take(remaining).collect();
            output.push_str(&partial);
            truncated = true;
            break;
        }

        output.push_str(&line_text);
        total_chars += line_chars;
    }

    (output, truncated)
}

fn truncate_output_line(line: &str, max_len: usize, line_break: &str) -> (String, bool) {
    let line_len = line.chars().count();
    if line_len <= max_len {
        let mut out = String::with_capacity(line.len() + line_break.len());
        out.push_str(line);
        out.push_str(line_break);
        return (out, false);
    }

    let marker_len = TRUNCATION_MARKER.chars().count();
    let max_len = max_len.max(marker_len);
    let take_len = max_len.saturating_sub(marker_len);
    let prefix: String = line.chars().take(take_len).collect();
    let mut out = String::new();
    out.push_str(&prefix);
    out.push_str(TRUNCATION_MARKER);
    out.push_str(line_break);
    (out, true)
}

fn append_truncation(summary: String, truncated: bool) -> String {
    if truncated {
        if summary.is_empty() {
            "Output is truncated to fit in the message.".to_string()
        } else if summary.ends_with('.') {
            format!("{summary} Output is truncated to fit in the message.")
        } else {
            format!("{summary}. Output is truncated to fit in the message.")
        }
    } else {
        summary
    }
}

pub fn tool_definitions() -> Vec<serde_json::Value> {
    vec![
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "ReadFile",
                "description": "Read the contents of a text file from disk.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to read." },
                        "line_offset": { "type": "integer", "description": "Line number to start from.", "minimum": 1 },
                        "n_lines": { "type": "integer", "description": "Number of lines to read.", "minimum": 1 }
                    },
                    "required": ["path"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "Shell",
                "description": "Run a shell command in the working directory.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "command": { "type": "string", "description": "Shell command to execute." },
                        "timeout": { "type": "integer", "description": "Timeout in seconds.", "minimum": 1 }
                    },
                    "required": ["command"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "WriteFile",
                "description": "Write content to a file (overwrite or append).",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to write." },
                        "content": { "type": "string", "description": "Content to write." },
                        "mode": { "type": "string", "enum": ["overwrite", "append"], "description": "Write mode." }
                    },
                    "required": ["path", "content"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "StrReplaceFile",
                "description": "Replace specific strings in a file.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "path": { "type": "string", "description": "File path to edit." },
                        "edit": {
                            "oneOf": [
                                {
                                    "type": "object",
                                    "properties": {
                                        "old": { "type": "string" },
                                        "new": { "type": "string" },
                                        "replace_all": { "type": "boolean" }
                                    },
                                    "required": ["old", "new"]
                                },
                                {
                                    "type": "array",
                                    "items": {
                                        "type": "object",
                                        "properties": {
                                            "old": { "type": "string" },
                                            "new": { "type": "string" },
                                            "replace_all": { "type": "boolean" }
                                        },
                                        "required": ["old", "new"]
                                    }
                                }
                            ]
                        }
                    },
                    "required": ["path", "edit"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "SearchWeb",
                "description": "Search the web using the configured search service.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "query": { "type": "string", "description": "Search query." },
                        "limit": { "type": "integer", "description": "Number of results.", "minimum": 1 },
                        "include_content": { "type": "boolean", "description": "Include page content in results." }
                    },
                    "required": ["query"]
                }
            }
        }),
        serde_json::json!({
            "type": "function",
            "function": {
                "name": "FetchURL",
                "description": "Fetch the contents of a URL.",
                "parameters": {
                    "type": "object",
                    "properties": {
                        "url": { "type": "string", "description": "URL to fetch." }
                    },
                    "required": ["url"]
                }
            }
        }),
    ]
}

pub fn read_file(
    work_dir: &str,
    path: &str,
    line_offset: usize,
    n_lines: usize,
) -> ToolOutput {
    let resolved = match resolve_path(work_dir, path, true) {
        Ok(p) => p,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: err,
                output: String::new(),
            }
        }
    };

    if !resolved.is_file() {
        return ToolOutput {
            ok: false,
            summary: "Path is not a file".to_string(),
            output: String::new(),
        };
    }

    let metadata = match fs::metadata(&resolved) {
        Ok(m) => m,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: format!("Failed to read file metadata: {err}"),
                output: String::new(),
            }
        }
    };

    if metadata.len() > MAX_BYTES as u64 {
        return ToolOutput {
            ok: false,
            summary: "File too large (max 100KB)".to_string(),
            output: String::new(),
        };
    }

    let file = match fs::File::open(&resolved) {
        Ok(f) => f,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: format!("Failed to read file: {err}"),
                output: String::new(),
            }
        }
    };

    let reader = io::BufReader::new(file);
    let mut lines = Vec::new();
    let mut truncated_lines = Vec::new();
    let mut total_bytes = 0usize;
    let mut line_no = 0usize;
    let start = line_offset.max(1);
    let max_lines = n_lines.max(1).min(MAX_LINES);

    for line in reader.lines() {
        let line = match line {
            Ok(line) => line,
            Err(_) => continue,
        };
        line_no += 1;
        if line_no < start {
            continue;
        }

        let (truncated, did_truncate) = truncate_line(&line);
        if did_truncate {
            truncated_lines.push(line_no);
        }

        total_bytes += truncated.len();
        lines.push((line_no, truncated));

        if lines.len() >= max_lines || total_bytes >= MAX_BYTES {
            break;
        }
    }

    let mut output = String::new();
    for (line_no, line) in &lines {
        output.push_str(&format!("{line_no:6}\t{line}\n"));
    }

    let mut summary = if lines.is_empty() {
        "No lines read from file.".to_string()
    } else {
        format!(
            "{} lines read from file starting at line {}.",
            lines.len(),
            start
        )
    };

    if lines.len() >= MAX_LINES {
        summary.push_str(" Max lines reached.");
    } else if total_bytes >= MAX_BYTES {
        summary.push_str(" Max bytes reached.");
    }

    if !truncated_lines.is_empty() {
        summary.push_str(&format!(" Lines {:?} were truncated.", truncated_lines));
    }

    ToolOutput {
        ok: true,
        summary,
        output,
    }
}

pub async fn run_shell(work_dir: &str, command: &str, timeout_secs: u64) -> ToolOutput {
    if command.trim().is_empty() {
        return ToolOutput {
            ok: false,
            summary: "Command cannot be empty".to_string(),
            output: String::new(),
        };
    }

    let (shell, args) = shell_command(command);
    let mut cmd = Command::new(shell);
    cmd.args(args).current_dir(work_dir);

    let result = timeout(Duration::from_secs(timeout_secs), cmd.output()).await;
    match result {
        Ok(Ok(output)) => {
            let stdout = String::from_utf8_lossy(&output.stdout).to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let mut combined = String::new();
            if !stdout.is_empty() {
                combined.push_str(&stdout);
            }
            if !stderr.is_empty() {
                if !combined.is_empty() && !combined.ends_with('\n') {
                    combined.push('\n');
                }
                combined.push_str(&stderr);
            }
            let (combined, truncated) = truncate_output(&combined);

            if output.status.success() {
                ToolOutput {
                    ok: true,
                    summary: append_truncation("Command executed successfully.".to_string(), truncated),
                    output: combined,
                }
            } else {
                ToolOutput {
                    ok: false,
                    summary: append_truncation(
                        format!("Command failed with exit code {:?}.", output.status.code()),
                        truncated,
                    ),
                    output: combined,
                }
            }
        }
        Ok(Err(err)) => ToolOutput {
            ok: false,
            summary: format!("Failed to execute command: {err}"),
            output: String::new(),
        },
        Err(_) => ToolOutput {
            ok: false,
            summary: format!("Command timed out after {timeout_secs} seconds."),
            output: String::new(),
        },
    }
}

fn shell_command(command: &str) -> (String, Vec<String>) {
    #[cfg(windows)]
    {
        return ("cmd".to_string(), vec!["/C".to_string(), command.to_string()]);
    }

    #[cfg(not(windows))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/bash".to_string());
        (shell, vec!["-lc".to_string(), command.to_string()])
    }
}

pub fn write_file(work_dir: &str, path: &str, content: &str, mode: &str) -> ToolOutput {
    let resolved = match resolve_path(work_dir, path, false) {
        Ok(p) => p,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: err,
                output: String::new(),
            }
        }
    };

    let parent = match resolved.parent() {
        Some(p) => p,
        None => {
            return ToolOutput {
                ok: false,
                summary: "Invalid file path".to_string(),
                output: String::new(),
            }
        }
    };

    if !parent.exists() {
        return ToolOutput {
            ok: false,
            summary: "Parent directory does not exist".to_string(),
            output: String::new(),
        };
    }

    match mode {
        "append" => {
            if let Err(err) = fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&resolved)
                .and_then(|mut file| {
                    use std::io::Write;
                    file.write_all(content.as_bytes())
                })
            {
                return ToolOutput {
                    ok: false,
                    summary: format!("Failed to append to file: {err}"),
                    output: String::new(),
                };
            }
        }
        _ => {
            if let Err(err) = fs::write(&resolved, content) {
                return ToolOutput {
                    ok: false,
                    summary: format!("Failed to write file: {err}"),
                    output: String::new(),
                };
            }
        }
    }

    let action = if mode == "append" { "appended to" } else { "overwritten" };
    ToolOutput {
        ok: true,
        summary: format!("File successfully {action}."),
        output: String::new(),
    }
}

#[derive(Debug, Deserialize)]
pub struct ReplaceEdit {
    pub old: String,
    pub new: String,
    #[serde(default)]
    pub replace_all: bool,
}

pub fn str_replace_file(
    work_dir: &str,
    path: &str,
    edits: Vec<ReplaceEdit>,
) -> ToolOutput {
    let resolved = match resolve_path(work_dir, path, true) {
        Ok(p) => p,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: err,
                output: String::new(),
            }
        }
    };

    if !resolved.is_file() {
        return ToolOutput {
            ok: false,
            summary: "Path is not a file".to_string(),
            output: String::new(),
        };
    }

    let original = match fs::read_to_string(&resolved) {
        Ok(c) => c,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: format!("Failed to read file: {err}"),
                output: String::new(),
            }
        }
    };

    let mut updated = original.clone();
    let mut total_replacements = 0usize;

    for edit in &edits {
        if edit.replace_all {
            let count = updated.matches(&edit.old).count();
            total_replacements += count;
            updated = updated.replace(&edit.old, &edit.new);
        } else if updated.contains(&edit.old) {
            updated = updated.replacen(&edit.old, &edit.new, 1);
            total_replacements += 1;
        }
    }

    if updated == original {
        return ToolOutput {
            ok: false,
            summary: "No replacements were made. The old string was not found.".to_string(),
            output: String::new(),
        };
    }

    if let Err(err) = fs::write(&resolved, updated) {
        return ToolOutput {
            ok: false,
            summary: format!("Failed to write file: {err}"),
            output: String::new(),
        };
    }

    ToolOutput {
        ok: true,
        summary: format!(
            "File successfully edited. Applied {} edit(s) with {} replacement(s).",
            edits.len(),
            total_replacements
        ),
        output: String::new(),
    }
}

pub async fn search_web(
    config_path: Option<&str>,
    tool_call_id: &str,
    query: &str,
    limit: usize,
    include_content: bool,
) -> ToolOutput {
    let config = match load_config_value(config_path) {
        Ok(value) => value,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: err,
                output: String::new(),
            }
        }
    };

    let service = match parse_service_config(&config, "moonshot_search") {
        Some(cfg) => cfg,
        None => {
            return ToolOutput {
                ok: false,
                summary: "Search service is not configured.".to_string(),
                output: String::new(),
            }
        }
    };

    let client = reqwest::Client::new();
    let mut req = client.post(&service.base_url);
    req = req.header("Authorization", format!("Bearer {}", service.api_key));
    req = req.header("X-Msh-Tool-Call-Id", tool_call_id);
    for (k, v) in common_headers().into_iter() {
        req = req.header(k, v);
    }
    for (k, v) in service.custom_headers.iter() {
        req = req.header(k, v);
    }

    let response = req
        .json(&serde_json::json!({
            "text_query": query,
            "limit": limit,
            "enable_page_crawling": include_content,
            "timeout_seconds": 30
        }))
        .send()
        .await;

    let response = match response {
        Ok(resp) => resp,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: format!("Failed to search: {err}"),
                output: String::new(),
            }
        }
    };

    if !response.status().is_success() {
        return ToolOutput {
            ok: false,
            summary: format!("Search request failed with status {}", response.status()),
            output: String::new(),
        };
    }

    let data: SearchResponse = match response.json().await {
        Ok(value) => value,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: format!("Failed to parse search response: {err}"),
                output: String::new(),
            }
        }
    };

    let mut output = String::new();
    for (i, result) in data.search_results.iter().enumerate() {
        if i > 0 {
            output.push_str("---\n\n");
        }
        output.push_str(&format!(
            "Title: {}\nDate: {}\nURL: {}\nSummary: {}\n\n",
            result.title,
            result.date,
            result.url,
            result.snippet
        ));
        if !result.content.is_empty() {
            output.push_str(&result.content);
            output.push_str("\n\n");
        }
    }
    let (output, truncated) = truncate_output(&output);
    ToolOutput {
        ok: true,
        summary: append_truncation("Search completed.".to_string(), truncated),
        output,
    }
}

pub async fn fetch_url(
    config_path: Option<&str>,
    tool_call_id: &str,
    url: &str,
) -> ToolOutput {
    let config = load_config_value(config_path).ok();
    if let Some(config) = config {
        if let Some(service) = parse_service_config(&config, "moonshot_fetch") {
            let client = reqwest::Client::new();
            let mut req = client.post(&service.base_url);
            req = req.header("Authorization", format!("Bearer {}", service.api_key));
            req = req.header("Accept", "text/markdown");
            req = req.header("X-Msh-Tool-Call-Id", tool_call_id);
            for (k, v) in common_headers().into_iter() {
                req = req.header(k, v);
            }
            for (k, v) in service.custom_headers.iter() {
                req = req.header(k, v);
            }

            if let Ok(response) = req.json(&serde_json::json!({ "url": url })).send().await {
                if response.status().is_success() {
                    if let Ok(text) = response.text().await {
                        let (output, truncated) = truncate_output(&text);
                        return ToolOutput {
                            ok: true,
                            summary: append_truncation(
                                "Fetched content via service.".to_string(),
                                truncated,
                            ),
                            output,
                        };
                    }
                }
            }
        }
    }

    let client = reqwest::Client::new();
    let response = match client
        .get(url)
        .header(
            "User-Agent",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 \
             (KHTML, like Gecko) Chrome/91.0.4472.124 Safari/537.36",
        )
        .send()
        .await
    {
        Ok(resp) => resp,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: format!("Failed to fetch URL: {err}"),
                output: String::new(),
            }
        }
    };

    if !response.status().is_success() {
        return ToolOutput {
            ok: false,
            summary: format!("Fetch failed with status {}", response.status()),
            output: String::new(),
        };
    }

    let content_type = response
        .headers()
        .get(CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_lowercase();

    let body = match response.text().await {
        Ok(text) => text,
        Err(err) => {
            return ToolOutput {
                ok: false,
                summary: format!("Failed to read response body: {err}"),
                output: String::new(),
            }
        }
    };

    let summary = if content_type.starts_with("text/plain")
        || content_type.starts_with("text/markdown")
    {
        "Fetched plain text content.".to_string()
    } else {
        "Fetched response body.".to_string()
    };
    let (output, truncated) = truncate_output(&body);

    ToolOutput {
        ok: true,
        summary: append_truncation(summary, truncated),
        output,
    }
}
