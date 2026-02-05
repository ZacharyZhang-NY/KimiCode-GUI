use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::Emitter;
use chrono::{DateTime, Utc};

// OAuth Constants
const KIMI_CODE_CLIENT_ID: &str = "17e5f671-d194-4dfb-9706-5516cb48c098";
const DEFAULT_OAUTH_HOST: &str = "https://auth.kimi.com";
const CREDENTIALS_FILE: &str = "kimi-code.json";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OAuthToken {
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: f64,
    pub scope: String,
    pub token_type: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DeviceAuthorization {
    pub user_code: String,
    pub device_code: String,
    pub verification_uri: String,
    pub verification_uri_complete: String,
    pub expires_in: Option<i32>,
    pub interval: i32,
}

#[derive(Clone, Debug, Serialize)]
pub struct OAuthEvent {
    pub event: String,
    pub data: serde_json::Value,
}

fn oauth_host() -> String {
    std::env::var("KIMI_CODE_OAUTH_HOST")
        .or_else(|_| std::env::var("KIMI_OAUTH_HOST"))
        .unwrap_or_else(|_| DEFAULT_OAUTH_HOST.to_string())
}

fn api_base_url() -> String {
    std::env::var("KIMI_CODE_BASE_URL")
        .or_else(|_| std::env::var("KIMI_BASE_URL"))
        .unwrap_or_else(|_| "https://api.kimi.com/coding/v1".to_string())
}

fn credentials_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let dir = home.join(".kimicodegui").join("credentials");
    std::fs::create_dir_all(&dir).ok();
    dir
}

fn credentials_path() -> PathBuf {
    credentials_dir().join(CREDENTIALS_FILE)
}

fn device_id_path() -> PathBuf {
    credentials_dir().join("device_id")
}

fn get_or_create_device_id() -> String {
    let path = device_id_path();
    if path.exists() {
        if let Ok(id) = std::fs::read_to_string(&path) {
            return id.trim().to_string();
        }
    }
    // Generate new device ID
    let device_id = uuid::Uuid::new_v4().to_string().replace("-", "");
    let _ = std::fs::write(&path, &device_id);
    device_id
}

pub fn common_headers() -> HashMap<String, String> {
    let mut headers = HashMap::new();
    // Identify as CLI coding agent to access restricted models
    headers.insert("User-Agent".to_string(), "KimiCLI/0.1.0".to_string());
    headers.insert("X-Msh-Platform".to_string(), "kimi_cli".to_string());
    headers.insert("X-Msh-Version".to_string(), "0.1.0".to_string());
    headers.insert(
        "X-Msh-Device-Name".to_string(),
        hostname::get()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
    );
    headers.insert(
        "X-Msh-Device-Model".to_string(),
        format!("{}", std::env::consts::ARCH),
    );
    headers.insert(
        "X-Msh-Os-Version".to_string(),
        format!("{} {}", std::env::consts::OS, std::env::consts::ARCH),
    );
    headers.insert("X-Msh-Device-Id".to_string(), get_or_create_device_id());
    headers
}

pub fn load_token() -> Option<OAuthToken> {
    let path = credentials_path();
    if !path.exists() {
        return None;
    }
    
    let content = std::fs::read_to_string(&path).ok()?;
    let value: serde_json::Value = serde_json::from_str(&content).ok()?;
    
    Some(OAuthToken {
        access_token: value.get("access_token")?.as_str()?.to_string(),
        refresh_token: value.get("refresh_token")?.as_str()?.to_string(),
        expires_at: value.get("expires_at")?.as_f64()?,
        scope: value.get("scope")?.as_str()?.to_string(),
        token_type: value.get("token_type")?.as_str()?.to_string(),
    })
}

pub fn save_token(token: &OAuthToken) -> Result<(), String> {
    let path = credentials_path();
    let json = serde_json::to_string_pretty(token)
        .map_err(|e| format!("Failed to serialize token: {}", e))?;
    
    let mut file = std::fs::File::create(&path)
        .map_err(|e| format!("Failed to create credentials file: {}", e))?;
    
    file.write_all(json.as_bytes())
        .map_err(|e| format!("Failed to write credentials: {}", e))?;
    
    // Set restrictive permissions (Unix only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).map_err(|e| e.to_string())?.permissions();
        perms.set_mode(0o600);
        std::fs::set_permissions(&path, perms).map_err(|e| e.to_string())?;
    }
    
    Ok(())
}

pub fn delete_token() {
    let path = credentials_path();
    if path.exists() {
        std::fs::remove_file(&path).ok();
    }
}

pub fn is_logged_in() -> bool {
    match load_token() {
        Some(token) => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            // Token is valid if not expired (with 5 min buffer)
            token.expires_at > now + 300.0
        }
        None => false,
    }
}

pub async fn request_device_authorization() -> Result<DeviceAuthorization, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/oauth/device_authorization", oauth_host());
    
    let form = [("client_id", KIMI_CODE_CLIENT_ID)];
    
    let response = client
        .post(&url)
        .form(&form)
        .headers(reqwest::header::HeaderMap::from_iter(
            common_headers()
                .iter()
                .map(|(k, v)| {
                    (
                        k.parse::<reqwest::header::HeaderName>().unwrap(),
                        v.parse().unwrap(),
                    )
                }),
        ))
        .send()
        .await
        .map_err(|e| format!("Request failed: {}", e))?;
    
    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Device authorization failed: {}", text));
    }
    
    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse response: {}", e))?;
    
    Ok(DeviceAuthorization {
        user_code: data["user_code"].as_str().unwrap_or("").to_string(),
        device_code: data["device_code"].as_str().unwrap_or("").to_string(),
        verification_uri: data["verification_uri"].as_str().unwrap_or("").to_string(),
        verification_uri_complete: data["verification_uri_complete"]
            .as_str()
            .unwrap_or("")
            .to_string(),
        expires_in: data["expires_in"].as_i64().map(|v| v as i32),
        interval: data["interval"].as_i64().map(|v| v as i32).unwrap_or(5),
    })
}

pub async fn poll_for_token(
    auth: &DeviceAuthorization,
    window: tauri::Window,
) -> Result<OAuthToken, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/oauth/token", oauth_host());
    let interval = std::time::Duration::from_secs(auth.interval.max(1) as u64);
    
    loop {
        tokio::time::sleep(interval).await;
        
        let form = [
            ("client_id", KIMI_CODE_CLIENT_ID),
            ("device_code", &auth.device_code),
            ("grant_type", "urn:ietf:params:oauth:grant-type:device_code"),
        ];
        
        let response = client
            .post(&url)
            .form(&form)
            .headers(reqwest::header::HeaderMap::from_iter(
                common_headers()
                    .iter()
                    .map(|(k, v)| {
                        (
                            k.parse::<reqwest::header::HeaderName>().unwrap(),
                            v.parse().unwrap(),
                        )
                    }),
            ))
            .send()
            .await
            .map_err(|e| format!("Token request failed: {}", e))?;
        
        let status = response.status();
        let data: serde_json::Value = response
            .json()
            .await
            .map_err(|e| format!("Failed to parse token response: {}", e))?;
        
        if status.is_success() && data.get("access_token").is_some() {
            let expires_in = data["expires_in"].as_f64().unwrap_or(3600.0);
            let expires_at = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64()
                + expires_in;
            
            return Ok(OAuthToken {
                access_token: data["access_token"].as_str().unwrap_or("").to_string(),
                refresh_token: data["refresh_token"].as_str().unwrap_or("").to_string(),
                expires_at,
                scope: data["scope"].as_str().unwrap_or("").to_string(),
                token_type: data["token_type"].as_str().unwrap_or("Bearer").to_string(),
            });
        }
        
        let error = data["error"].as_str().unwrap_or("unknown_error");
        
        match error {
            "authorization_pending" => {
                // Still waiting, emit waiting event
                let _ = window.emit(
                    "oauth://event",
                    OAuthEvent {
                        event: "waiting".to_string(),
                        data: serde_json::json!({
                            "message": "Waiting for authorization...",
                        }),
                    },
                );
                continue;
            }
            "expired_token" => {
                return Err("Device code expired. Please try again.".to_string());
            }
            "access_denied" => {
                return Err("Authorization was denied.".to_string());
            }
            _ => {
                let error_desc = data["error_description"].as_str().unwrap_or(error);
                return Err(format!("Authorization failed: {}", error_desc));
            }
        }
    }
}

pub async fn refresh_token(refresh_token: &str) -> Result<OAuthToken, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/api/oauth/token", oauth_host());
    
    let form = [
        ("client_id", KIMI_CODE_CLIENT_ID),
        ("grant_type", "refresh_token"),
        ("refresh_token", refresh_token),
    ];
    
    let response = client
        .post(&url)
        .form(&form)
        .headers(reqwest::header::HeaderMap::from_iter(
            common_headers()
                .iter()
                .map(|(k, v)| {
                    (
                        k.parse::<reqwest::header::HeaderName>().unwrap(),
                        v.parse().unwrap(),
                    )
                }),
        ))
        .send()
        .await
        .map_err(|e| format!("Refresh request failed: {}", e))?;
    
    if !response.status().is_success() {
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Token refresh failed: {}", text));
    }
    
    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|e| format!("Failed to parse refresh response: {}", e))?;
    
    let expires_in = data["expires_in"].as_f64().unwrap_or(3600.0);
    let expires_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64()
        + expires_in;
    
    Ok(OAuthToken {
        access_token: data["access_token"].as_str().unwrap_or("").to_string(),
        refresh_token: data["refresh_token"]
            .as_str()
            .unwrap_or(refresh_token)
            .to_string(),
        expires_at,
        scope: data["scope"].as_str().unwrap_or("").to_string(),
        token_type: data["token_type"].as_str().unwrap_or("Bearer").to_string(),
    })
}

pub async fn ensure_fresh_token() -> Option<String> {
    let token = load_token()?;
    
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    
    // If token expires in less than 5 minutes, refresh it
    if token.expires_at < now + 300.0 {
        match refresh_token(&token.refresh_token).await {
            Ok(new_token) => {
                let access_token = new_token.access_token.clone();
                save_token(&new_token).ok()?;
                Some(access_token)
            }
            Err(_) => {
                // Refresh failed, try using existing token
                Some(token.access_token)
            }
        }
    } else {
        Some(token.access_token)
    }
}

#[tauri::command]
pub fn oauth_check_status() -> Result<serde_json::Value, String> {
    let is_logged_in = is_logged_in();
    let token_info = load_token();
    
    Ok(serde_json::json!({
        "is_logged_in": is_logged_in,
        "has_token": token_info.is_some(),
    }))
}

#[tauri::command]
pub fn oauth_logout() -> Result<(), String> {
    delete_token();
    Ok(())
}

#[tauri::command]
pub async fn oauth_start_login(window: tauri::Window) -> Result<serde_json::Value, String> {
    let auth = request_device_authorization().await?;
    
    // Clone values for the response
    let user_code = auth.user_code.clone();
    let verification_uri = auth.verification_uri.clone();
    let verification_uri_complete = auth.verification_uri_complete.clone();
    
    // Spawn polling task - clone auth for the async task
    let window_clone = window.clone();
    let auth_for_poll = auth.clone();
    
    tokio::spawn(async move {
        match poll_for_token(&auth_for_poll, window_clone.clone()).await {
            Ok(token) => {
                if let Err(e) = save_token(&token) {
                    let _ = window_clone.emit(
                        "oauth://event",
                        OAuthEvent {
                            event: "error".to_string(),
                            data: serde_json::json!({ "message": e }),
                        },
                    );
                    return;
                }
                
                let _ = window_clone.emit(
                    "oauth://event",
                    OAuthEvent {
                        event: "success".to_string(),
                        data: serde_json::json!({}),
                    },
                );
            }
            Err(e) => {
                let _ = window_clone.emit(
                    "oauth://event",
                    OAuthEvent {
                        event: "error".to_string(),
                        data: serde_json::json!({ "message": e }),
                    },
                );
            }
        }
    });
    
    Ok(serde_json::json!({
        "user_code": user_code,
        "verification_uri": verification_uri,
        "verification_uri_complete": verification_uri_complete,
    }))
}

#[tauri::command]
pub async fn oauth_open_browser(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| format!("Failed to open browser: {}", e))
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct UserProfile {
    pub total_label: String,
    pub total_percent: f64,
    pub total_reset: String,
    pub limit_label: String,
    pub limit_percent: f64,
    pub limit_reset: String,
}

#[derive(Debug, Clone)]
struct UsageSummary {
    used: i64,
    limit: i64,
    reset: Option<String>,
}

fn to_i64(value: &serde_json::Value) -> Option<i64> {
    match value {
        serde_json::Value::Number(num) => num.as_i64().or_else(|| num.as_u64().map(|v| v as i64)),
        serde_json::Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn format_duration(mut seconds: i64) -> String {
    if seconds < 0 {
        seconds = 0;
    }
    let days = seconds / 86_400;
    seconds %= 86_400;
    let hours = seconds / 3_600;
    seconds %= 3_600;
    let minutes = seconds / 60;
    let mut parts = Vec::new();
    if days > 0 {
        parts.push(format!("{days}d"));
    }
    if hours > 0 {
        parts.push(format!("{hours}h"));
    }
    if minutes > 0 || parts.is_empty() {
        parts.push(format!("{minutes}m"));
    }
    parts.join(" ")
}

fn normalize_rfc3339(value: &str) -> String {
    if value.ends_with('Z') {
        if let Some((base, frac)) = value.trim_end_matches('Z').split_once('.') {
            let truncated = &frac[..frac.len().min(6)];
            return format!("{base}.{truncated}Z");
        }
    }
    value.to_string()
}

fn reset_hint(map: &serde_json::Map<String, serde_json::Value>) -> Option<String> {
    for key in ["reset_at", "resetAt", "reset_time", "resetTime"] {
        if let Some(value) = map.get(key).and_then(|v| v.as_str()) {
            let normalized = normalize_rfc3339(value);
            if let Ok(parsed) = DateTime::parse_from_rfc3339(&normalized) {
                let now = Utc::now();
                let delta = parsed.with_timezone(&Utc) - now;
                let seconds = delta.num_seconds();
                if seconds > 0 {
                    return Some(format!("resets in {}", format_duration(seconds)));
                }
                return Some("reset".to_string());
            }
        }
    }

    for key in ["reset_in", "resetIn", "ttl", "window"] {
        if let Some(value) = map.get(key) {
            if let Some(seconds) = to_i64(value) {
                if seconds > 0 {
                    return Some(format!("resets in {}", format_duration(seconds)));
                }
            }
        }
    }

    None
}

fn usage_summary(payload: &serde_json::Value) -> Option<UsageSummary> {
    let usage = payload.get("usage")?.as_object()?;
    let limit = usage.get("limit").and_then(to_i64);
    let used = usage.get("used").and_then(to_i64).or_else(|| {
        let remaining = usage.get("remaining").and_then(to_i64);
        match (limit, remaining) {
            (Some(limit), Some(remaining)) => Some(limit - remaining),
            _ => None,
        }
    });

    let limit = limit?;
    let used = used.unwrap_or(0);
    let reset = reset_hint(usage);
    Some(UsageSummary { used, limit, reset })
}

#[derive(Debug, Clone)]
struct UsageRow {
    label: String,
    used: i64,
    limit: i64,
    reset: Option<String>,
}

fn limit_label(
    item: &serde_json::Map<String, serde_json::Value>,
    detail: &serde_json::Map<String, serde_json::Value>,
    window: &serde_json::Map<String, serde_json::Value>,
    idx: usize,
) -> String {
    for key in ["name", "title", "scope"] {
        if let Some(value) = item.get(key).and_then(|v| v.as_str()) {
            let label = value.trim();
            if !label.is_empty() {
                return label.to_string();
            }
        }
        if let Some(value) = detail.get(key).and_then(|v| v.as_str()) {
            let label = value.trim();
            if !label.is_empty() {
                return label.to_string();
            }
        }
    }

    let duration = window
        .get("duration")
        .and_then(to_i64)
        .or_else(|| item.get("duration").and_then(to_i64))
        .or_else(|| detail.get("duration").and_then(to_i64));
    let time_unit = window
        .get("timeUnit")
        .and_then(|v| v.as_str())
        .or_else(|| item.get("timeUnit").and_then(|v| v.as_str()))
        .or_else(|| detail.get("timeUnit").and_then(|v| v.as_str()))
        .unwrap_or("");

    if let Some(duration) = duration {
        if time_unit.contains("MINUTE") {
            if duration >= 60 && duration % 60 == 0 {
                return format!("{}h limit", duration / 60);
            }
            return format!("{duration}m limit");
        }
        if time_unit.contains("HOUR") {
            return format!("{duration}h limit");
        }
        if time_unit.contains("DAY") {
            return format!("{duration}d limit");
        }
        return format!("{duration}s limit");
    }

    format!("Limit #{}", idx + 1)
}

fn usage_row_from_maps(
    label: String,
    detail: &serde_json::Map<String, serde_json::Value>,
    item: &serde_json::Map<String, serde_json::Value>,
    window: &serde_json::Map<String, serde_json::Value>,
) -> Option<UsageRow> {
    let limit = detail.get("limit").and_then(to_i64);
    let used = detail.get("used").and_then(to_i64).or_else(|| {
        let remaining = detail.get("remaining").and_then(to_i64);
        match (limit, remaining) {
            (Some(limit), Some(remaining)) => Some(limit - remaining),
            _ => None,
        }
    });

    let limit = limit?;
    let used = used.unwrap_or(0);
    let reset = reset_hint(detail)
        .or_else(|| reset_hint(item))
        .or_else(|| reset_hint(window));

    Some(UsageRow {
        label,
        used,
        limit,
        reset,
    })
}

fn usage_limits(payload: &serde_json::Value) -> Vec<UsageRow> {
    let mut rows = Vec::new();
    let limits = match payload.get("limits").and_then(|v| v.as_array()) {
        Some(limits) => limits,
        None => return rows,
    };

    for (idx, item) in limits.iter().enumerate() {
        let item_obj = match item.as_object() {
            Some(obj) => obj,
            None => continue,
        };
        let detail_obj = item_obj
            .get("detail")
            .and_then(|v| v.as_object())
            .unwrap_or(item_obj);
        let empty_window = serde_json::Map::new();
        let window_obj = item_obj
            .get("window")
            .and_then(|v| v.as_object())
            .unwrap_or(&empty_window);
        let label = limit_label(item_obj, detail_obj, window_obj, idx);
        if let Some(row) = usage_row_from_maps(label, detail_obj, item_obj, window_obj) {
            rows.push(row);
        }
    }

    rows
}

fn find_five_hour_limit(rows: &[UsageRow]) -> Option<UsageRow> {
    for row in rows {
        let label = row.label.to_lowercase();
        if label.contains("5h") || label.contains("5 h") {
            return Some(row.clone());
        }
    }
    None
}

async fn fetch_usage_payload(access_token: &str) -> Result<serde_json::Value, String> {
    let client = reqwest::Client::new();
    let url = format!("{}/usages", api_base_url().trim_end_matches('/'));

    let mut req = client.get(&url);
    for (key, value) in common_headers().into_iter() {
        req = req.header(key, value);
    }
    req = req.header("Authorization", format!("Bearer {}", access_token));

    let response = req
        .send()
        .await
        .map_err(|e| format!("Usage request failed: {}", e))?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(format!("Usage request failed: {} {}", status, text));
    }

    response
        .json()
        .await
        .map_err(|e| format!("Failed to parse usage response: {}", e))
}

#[tauri::command]
pub async fn oauth_get_user() -> Result<UserProfile, String> {
    let access_token = ensure_fresh_token()
        .await
        .ok_or_else(|| "Not logged in".to_string())?;
    let payload = fetch_usage_payload(&access_token).await?;
    let summary =
        usage_summary(&payload).ok_or_else(|| "Usage summary unavailable".to_string())?;
    let limits = usage_limits(&payload);
    let five_hour =
        find_five_hour_limit(&limits).ok_or_else(|| "5h limit unavailable".to_string())?;

    let total_percent = if summary.limit > 0 {
        (summary.used as f64 / summary.limit as f64) * 100.0
    } else {
        0.0
    };
    let limit_percent = if five_hour.limit > 0 {
        (five_hour.used as f64 / five_hour.limit as f64) * 100.0
    } else {
        0.0
    };

    Ok(UserProfile {
        total_label: "Weekly usage".to_string(),
        total_percent,
        total_reset: summary.reset.unwrap_or_default(),
        limit_label: "Rate limit".to_string(),
        limit_percent,
        limit_reset: five_hour.reset.unwrap_or_default(),
    })
}
