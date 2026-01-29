use serde::Deserialize;
use std::io::{BufRead, BufReader};
use std::process::{Command, Stdio};
use tauri::Emitter;

#[derive(Clone, serde::Serialize)]
pub struct CliStreamEvent {
    pub event: String,
    pub data: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct WireMessage {
    #[serde(rename = "type")]
    msg_type: String,
    #[serde(flatten)]
    extra: serde_json::Value,
}

pub async fn stream_cli_chat(
    window: tauri::Window,
    session_id: String,
    message: String,
    cli_path: Option<String>,
    work_dir: String,
    model: Option<String>,
    thinking: bool,
    mut cancel_rx: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), String> {
    let cli_cmd = find_cli(cli_path)?;
    
    let mut cmd = Command::new(&cli_cmd);
    cmd.arg("--wire")
        .arg("--prompt")
        .arg(&message)
        .arg("--work-dir")
        .arg(&work_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    
    if let Some(m) = model {
        cmd.arg("--model").arg(m);
    }
    
    if thinking {
        cmd.arg("--thinking");
    }
    
    // Try to resume session if provided
    if !session_id.is_empty() && session_id.len() > 8 {
        cmd.arg("--session").arg(&session_id);
    }
    
    let mut child = cmd.spawn()
        .map_err(|e| format!("Failed to spawn CLI: {}. Make sure kimi is installed.", e))?;
    
    let stdout = child.stdout.take()
        .ok_or("Failed to capture stdout")?;
    
    let reader = BufReader::new(stdout);
    let window_clone = window.clone();
    let session_id_clone = session_id.clone();
    
    // Spawn blocking read in a separate thread
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(100);
    
    std::thread::spawn(move || {
        for line in reader.lines().flatten() {
            if tx.blocking_send(line).is_err() {
                break;
            }
        }
    });
    
    // Process lines with cancellation support
    loop {
        tokio::select! {
            line = rx.recv() => {
                match line {
                    Some(line) => {
                        process_wire_line(&window_clone, &session_id_clone, &line);
                    }
                    None => {
                        // Stream ended
                        let _ = window_clone.emit("chat://event", CliStreamEvent {
                            event: "done".to_string(),
                            data: serde_json::json!({ "session_id": session_id_clone }),
                        });
                        break;
                    }
                }
            }
            _ = &mut cancel_rx => {
                let _ = child.kill();
                let _ = window.emit("chat://event", CliStreamEvent {
                    event: "cancelled".to_string(),
                    data: serde_json::json!({ "session_id": session_id }),
                });
                return Ok(());
            }
        }
    }
    
    // Wait for child to exit
    let _ = child.wait();
    
    Ok(())
}

fn process_wire_line(window: &tauri::Window, session_id: &str, line: &str) {
    if line.trim().is_empty() {
        return;
    }
    
    match serde_json::from_str::<WireMessage>(line) {
        Ok(msg) => {
            match msg.msg_type.as_str() {
                "TurnBegin" | "turn_begin" => {
                    // New turn started
                }
                "TextPart" | "text_part" => {
                    if let Some(content) = msg.extra.get("content").and_then(|v| v.as_str()) {
                        let _ = window.emit("chat://event", CliStreamEvent {
                            event: "chunk".to_string(),
                            data: serde_json::json!({
                                "session_id": session_id,
                                "content": content,
                            }),
                        });
                    }
                }
                "ThinkPart" | "think_part" => {
                    // Thinking content - could be displayed separately
                }
                "ToolCall" | "tool_call" => {
                    let _ = window.emit("chat://event", CliStreamEvent {
                        event: "tool_call".to_string(),
                        data: serde_json::json!({
                            "session_id": session_id,
                            "data": msg.extra,
                        }),
                    });
                }
                "ToolResult" | "tool_result" => {
                    let _ = window.emit("chat://event", CliStreamEvent {
                        event: "tool_result".to_string(),
                        data: serde_json::json!({
                            "session_id": session_id,
                            "data": msg.extra,
                        }),
                    });
                }
                "StepBegin" | "step_begin" => {
                    let _ = window.emit("chat://event", CliStreamEvent {
                        event: "step_begin".to_string(),
                        data: serde_json::json!({ "session_id": session_id }),
                    });
                }
                "StepEnd" | "step_end" | "TurnEnd" | "turn_end" => {
                    let _ = window.emit("chat://event", CliStreamEvent {
                        event: "step_end".to_string(),
                        data: serde_json::json!({ "session_id": session_id }),
                    });
                }
                "Error" | "error" => {
                    let error_msg = msg.extra.get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown error");
                    let _ = window.emit("chat://event", CliStreamEvent {
                        event: "error".to_string(),
                        data: serde_json::json!({
                            "session_id": session_id,
                            "message": error_msg,
                        }),
                    });
                }
                _ => {}
            }
        }
        Err(e) => {
            // Not a valid wire message, treat as plain text
            let _ = window.emit("chat://event", CliStreamEvent {
                event: "chunk".to_string(),
                data: serde_json::json!({
                    "session_id": session_id,
                    "content": line,
                }),
            });
        }
    }
}

fn find_cli(cli_path: Option<String>) -> Result<String, String> {
    // 1. Check explicit path
    if let Some(path) = cli_path {
        if !path.is_empty() {
            if std::path::Path::new(&path).exists() {
                return Ok(path);
            }
            return Err(format!("Configured CLI path not found: {}", path));
        }
    }
    
    // 2. Check KIMI_GUI_COMMAND env
    if let Ok(cmd) = std::env::var("KIMI_GUI_COMMAND") {
        if let Ok(parts) = shell_words::split(&cmd) {
            if !parts.is_empty() {
                return Ok(parts[0].clone());
            }
        }
    }
    
    // 3. Check PATH for kimi/kimi-cli
    let names = ["kimi", "kimi-cli"];
    if let Some(path) = find_in_path(&names) {
        return Ok(path.to_string_lossy().to_string());
    }
    
    // 4. Try python module
    let python_names = ["python3", "python"];
    if let Some(python) = find_in_path(&python_names) {
        return Ok(python.to_string_lossy().to_string());
    }
    
    Err("Kimi CLI not found. Please install kimi-cli or configure API mode.".to_string())
}

fn find_in_path(names: &[&str]) -> Option<std::path::PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
            #[cfg(windows)]
            {
                let exe = dir.join(format!("{name}.exe"));
                if exe.is_file() {
                    return Some(exe);
                }
                let cmd = dir.join(format!("{name}.cmd"));
                if cmd.is_file() {
                    return Some(cmd);
                }
            }
        }
    }
    None
}

#[tauri::command]
pub fn check_cli_available(cli_path: Option<String>) -> Result<bool, String> {
    match find_cli(cli_path) {
        Ok(cmd) => {
            // Verify it's actually working
            match Command::new(&cmd).arg("--version").output() {
                Ok(output) => Ok(output.status.success()),
                Err(_) => Ok(false),
            }
        }
        Err(_) => Ok(false),
    }
}

#[tauri::command]
pub fn get_cli_version(cli_path: Option<String>) -> Result<String, String> {
    let cmd = find_cli(cli_path)?;
    let output = Command::new(&cmd)
        .arg("--version")
        .output()
        .map_err(|e| format!("Failed to run CLI: {}", e))?;
    
    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout);
        Ok(version.trim().to_string())
    } else {
        let err = String::from_utf8_lossy(&output.stderr);
        Err(format!("CLI error: {}", err))
    }
}
