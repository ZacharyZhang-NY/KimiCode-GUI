use std::env;
use std::fs;
use std::io::Read;
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

fn find_in_path(names: &[&str]) -> Option<PathBuf> {
    let path_var = env::var_os("PATH")?;
    for dir in env::split_paths(&path_var) {
        for name in names {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
    }
    None
}

fn target_binary_filename(target_os: &str, target_arch: &str) -> Option<String> {
    let platform = node_platform(target_os)?;
    let arch = node_arch(target_arch)?;
    let ext = if target_os == "windows" { ".exe" } else { "" };
    Some(format!("agent-browser-{platform}-{arch}{ext}"))
}

fn is_node_wrapper(path: &PathBuf) -> bool {
    let mut file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return false,
    };

    let mut buf = [0u8; 256];
    let len = match file.read(&mut buf) {
        Ok(len) => len,
        Err(_) => return false,
    };
    let head = &buf[..len];

    head.starts_with(b"#!/usr/bin/env node")
        || head.starts_with(b"#! /usr/bin/env node")
        || head.starts_with(b"#!/usr/bin/node")
}

fn normalize_agent_browser_candidate(
    candidate: PathBuf,
    target_os: &str,
    target_arch: &str,
) -> Option<PathBuf> {
    if !candidate.is_file() {
        return None;
    }

    let Some(expected_name) = target_binary_filename(target_os, target_arch) else {
        return Some(candidate);
    };

    let candidate_name = candidate
        .file_name()
        .map(|name| name.to_string_lossy().to_string());

    if candidate_name.as_deref() == Some(expected_name.as_str()) {
        return Some(candidate);
    }

    // If PATH resolves to the Node launcher script, prefer the sibling native binary.
    if is_node_wrapper(&candidate) {
        if let Some(parent) = candidate.parent() {
            let sibling = parent.join(expected_name);
            if sibling.is_file() {
                return Some(sibling);
            }
        }
        return None;
    }

    Some(candidate)
}

fn parse_truthy_env(name: &str) -> bool {
    match env::var(name) {
        Ok(value) => matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        ),
        Err(_) => false,
    }
}

fn node_platform(target_os: &str) -> Option<&'static str> {
    match target_os {
        "macos" => Some("darwin"),
        "linux" => Some("linux"),
        "windows" => Some("win32"),
        _ => None,
    }
}

fn node_arch(target_arch: &str) -> Option<&'static str> {
    match target_arch {
        "x86_64" => Some("x64"),
        "aarch64" => Some("arm64"),
        _ => None,
    }
}

fn resolve_agent_browser_from_dir(target_os: &str, target_arch: &str) -> Option<PathBuf> {
    let base_dir = env::var_os("AGENT_BROWSER_BIN_DIR")?;
    let base_dir = PathBuf::from(base_dir);
    let filename = target_binary_filename(target_os, target_arch)?;

    let candidate = base_dir.join(filename);
    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

fn resolve_agent_browser_from_local_node_modules(
    target_os: &str,
    target_arch: &str,
) -> Option<PathBuf> {
    let manifest_dir = env::var_os("CARGO_MANIFEST_DIR")?;
    let manifest_dir = PathBuf::from(manifest_dir);
    let filename = target_binary_filename(target_os, target_arch)?;

    let candidate = manifest_dir
        .parent()
        .map(PathBuf::from)
        .unwrap_or(manifest_dir)
        .join("node_modules")
        .join("agent-browser")
        .join("bin")
        .join(filename);

    if candidate.is_file() {
        Some(candidate)
    } else {
        None
    }
}

fn resolve_agent_browser_binary(target_os: &str, target_arch: &str) -> Option<PathBuf> {
    if let Some(path) = env::var_os("AGENT_BROWSER_BIN") {
        let binary = PathBuf::from(path);
        if binary.is_file() {
            if let Some(normalized) =
                normalize_agent_browser_candidate(binary, target_os, target_arch)
            {
                return Some(normalized);
            }
        }
    }

    if let Some(binary) = resolve_agent_browser_from_dir(target_os, target_arch) {
        return Some(binary);
    }

    if let Some(binary) = resolve_agent_browser_from_local_node_modules(target_os, target_arch) {
        return Some(binary);
    }

    let mut dynamic_names: Vec<String> = Vec::new();
    if let Some(expected) = target_binary_filename(target_os, target_arch) {
        dynamic_names.push(expected);
    }
    #[cfg(windows)]
    {
        dynamic_names.push("agent-browser.exe".to_string());
    }
    dynamic_names.push("agent-browser".to_string());

    let names: Vec<&str> = dynamic_names.iter().map(|name| name.as_str()).collect();

    find_in_path(&names)
        .and_then(|candidate| normalize_agent_browser_candidate(candidate, target_os, target_arch))
}

fn prepare_embedded_agent_browser() {
    println!("cargo:rerun-if-env-changed=AGENT_BROWSER_BIN");
    println!("cargo:rerun-if-env-changed=AGENT_BROWSER_BIN_DIR");
    println!("cargo:rerun-if-env-changed=AGENT_BROWSER_REQUIRED");

    let target_os = env::var("CARGO_CFG_TARGET_OS").unwrap_or_else(|_| "unknown".to_string());
    let target_arch = env::var("CARGO_CFG_TARGET_ARCH").unwrap_or_else(|_| "unknown".to_string());
    let target_key = format!("{target_os}-{target_arch}");
    let required = parse_truthy_env("AGENT_BROWSER_REQUIRED");

    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()),
    );
    let resources_dir = manifest_dir
        .join("resources")
        .join("agent-browser")
        .join(&target_key);

    let _ = fs::create_dir_all(&resources_dir);

    let binary_name = if target_os == "windows" {
        "agent-browser.exe"
    } else {
        "agent-browser"
    };
    let destination = resources_dir.join(binary_name);

    let source = match resolve_agent_browser_binary(&target_os, &target_arch) {
        Some(path) => path,
        None => {
            let _ = fs::remove_file(&destination);
            if required {
                panic!(
                    "AGENT_BROWSER_REQUIRED is enabled, but no agent-browser binary was found for target {}. Set AGENT_BROWSER_BIN or AGENT_BROWSER_BIN_DIR.",
                    target_key
                );
            }
            return;
        }
    };

    if source != destination {
        fs::copy(&source, &destination).unwrap_or_else(|error| {
            panic!(
                "Failed to copy bundled agent-browser from {} to {}: {}",
                source.to_string_lossy(),
                destination.to_string_lossy(),
                error
            )
        });
    }

    #[cfg(unix)]
    {
        let metadata = fs::metadata(&destination).unwrap_or_else(|error| {
            panic!(
                "Failed to read metadata for bundled agent-browser at {}: {}",
                destination.to_string_lossy(),
                error
            )
        });
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&destination, permissions).unwrap_or_else(|error| {
            panic!(
                "Failed to set executable permission for bundled agent-browser at {}: {}",
                destination.to_string_lossy(),
                error
            )
        });
    }
}

fn main() {
    prepare_embedded_agent_browser();
    tauri_build::build();
}
