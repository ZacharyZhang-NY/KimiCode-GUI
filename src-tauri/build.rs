use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::collections::{HashMap, HashSet};

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

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!(
            "Source directory does not exist: {}",
            source.to_string_lossy()
        ));
    }

    fs::create_dir_all(destination).map_err(|error| {
        format!(
            "Failed to create directory {}: {}",
            destination.to_string_lossy(),
            error
        )
    })?;

    let entries = fs::read_dir(source).map_err(|error| {
        format!(
            "Failed to read directory {}: {}",
            source.to_string_lossy(),
            error
        )
    })?;

    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "Failed to read entry in {}: {}",
                source.to_string_lossy(),
                error
            )
        })?;
        let source_path = entry.path();
        let destination_path = destination.join(entry.file_name());

        if source_path.is_dir() {
            copy_dir_recursive(&source_path, &destination_path)?;
        } else if source_path.is_file() {
            if let Some(parent) = destination_path.parent() {
                fs::create_dir_all(parent).map_err(|error| {
                    format!(
                        "Failed to create parent {}: {}",
                        parent.to_string_lossy(),
                        error
                    )
                })?;
            }
            fs::copy(&source_path, &destination_path).map_err(|error| {
                format!(
                    "Failed to copy {} -> {}: {}",
                    source_path.to_string_lossy(),
                    destination_path.to_string_lossy(),
                    error
                )
            })?;
        }
    }

    Ok(())
}

fn read_json_object(path: &Path) -> Result<HashMap<String, serde_json::Value>, String> {
    let content = fs::read_to_string(path).map_err(|error| {
        format!("Failed to read JSON file {}: {}", path.to_string_lossy(), error)
    })?;

    serde_json::from_str::<HashMap<String, serde_json::Value>>(&content).map_err(|error| {
        format!(
            "Failed to parse JSON object in {}: {}",
            path.to_string_lossy(),
            error
        )
    })
}

fn read_dependency_names(package_dir: &Path) -> Vec<String> {
    let package_json_path = package_dir.join("package.json");
    let package_json = match read_json_object(&package_json_path) {
        Ok(value) => value,
        Err(_) => return Vec::new(),
    };

    let mut deps = Vec::new();
    for key in ["dependencies", "optionalDependencies"] {
        if let Some(serde_json::Value::Object(entries)) = package_json.get(key) {
            deps.extend(entries.keys().cloned());
        }
    }
    deps
}

fn resolve_dependency_dir(
    from_package_dir: &Path,
    dependency_name: &str,
    workspace_root: &Path,
) -> Option<PathBuf> {
    let mut current = Some(from_package_dir);
    while let Some(dir) = current {
        let candidate = dir.join("node_modules").join(dependency_name);
        if candidate.is_dir() {
            return Some(candidate);
        }
        if dir == workspace_root {
            break;
        }
        current = dir.parent();
    }

    let fallback = workspace_root.join("node_modules").join(dependency_name);
    if fallback.is_dir() {
        Some(fallback)
    } else {
        None
    }
}

fn copy_agent_browser_runtime_closure(
    package_dir: &Path,
    workspace_root: &Path,
    runtime_root: &Path,
    visited: &mut HashSet<PathBuf>,
) -> Result<(), String> {
    let canonical_package_dir = fs::canonicalize(package_dir).map_err(|error| {
        format!(
            "Failed to canonicalize package dir {}: {}",
            package_dir.to_string_lossy(),
            error
        )
    })?;

    if !visited.insert(canonical_package_dir.clone()) {
        return Ok(());
    }

    let relative = canonical_package_dir
        .strip_prefix(workspace_root)
        .map_err(|_| {
            format!(
                "Package dir {} is outside workspace root {}",
                canonical_package_dir.to_string_lossy(),
                workspace_root.to_string_lossy()
            )
        })?;
    let destination_dir = runtime_root.join(relative);
    copy_dir_recursive(&canonical_package_dir, &destination_dir)?;

    // Keep runtime lean and avoid unsigned extra platform binaries in notarized bundles.
    if canonical_package_dir
        .file_name()
        .map(|name| name == "agent-browser")
        .unwrap_or(false)
    {
        for removable in ["bin", "skills", "src", "scripts", "docker", "assets"] {
            let removable_path = destination_dir.join(removable);
            if removable_path.exists() {
                let _ = fs::remove_dir_all(&removable_path);
            }
        }
    }

    for dependency_name in read_dependency_names(&canonical_package_dir) {
        if let Some(dependency_dir) =
            resolve_dependency_dir(&canonical_package_dir, &dependency_name, workspace_root)
        {
            copy_agent_browser_runtime_closure(
                &dependency_dir,
                workspace_root,
                runtime_root,
                visited,
            )?;
        }
    }

    Ok(())
}

fn prepare_agent_browser_runtime(
    resources_dir: &Path,
    target_key: &str,
    required: bool,
) -> Result<Option<PathBuf>, String> {
    let manifest_dir = PathBuf::from(
        env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_string()),
    );
    let workspace_root = manifest_dir
        .parent()
        .map(PathBuf::from)
        .unwrap_or(manifest_dir.clone());

    let source_package_dir = workspace_root.join("node_modules").join("agent-browser");
    if !source_package_dir.is_dir() {
        if required {
            return Err(format!(
                "AGENT_BROWSER_REQUIRED is enabled, but package not found at {}",
                source_package_dir.to_string_lossy()
            ));
        }
        return Ok(None);
    }

    let runtime_root = resources_dir.join("agent-browser").join(target_key).join("runtime");
    if runtime_root.exists() {
        fs::remove_dir_all(&runtime_root).map_err(|error| {
            format!(
                "Failed to clear existing runtime dir {}: {}",
                runtime_root.to_string_lossy(),
                error
            )
        })?;
    }
    fs::create_dir_all(&runtime_root).map_err(|error| {
        format!(
            "Failed to create runtime dir {}: {}",
            runtime_root.to_string_lossy(),
            error
        )
    })?;

    let mut visited = HashSet::new();
    copy_agent_browser_runtime_closure(
        &source_package_dir,
        &workspace_root,
        &runtime_root,
        &mut visited,
    )?;

    let daemon_path = runtime_root
        .join("node_modules")
        .join("agent-browser")
        .join("dist")
        .join("daemon.js");
    if !daemon_path.is_file() {
        if required {
            return Err(format!(
                "AGENT_BROWSER_REQUIRED is enabled, but daemon.js is missing at {}",
                daemon_path.to_string_lossy()
            ));
        }
        return Ok(None);
    }

    Ok(Some(
        runtime_root
            .join("node_modules")
            .join("agent-browser"),
    ))
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
    println!("cargo:rerun-if-changed=../package-lock.json");
    println!("cargo:rerun-if-changed=../node_modules/agent-browser/package.json");

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

    if let Err(error) = prepare_agent_browser_runtime(
        &manifest_dir.join("resources"),
        &target_key,
        required,
    ) {
        if required {
            panic!("{error}");
        } else {
            eprintln!("cargo:warning={error}");
        }
    }

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
