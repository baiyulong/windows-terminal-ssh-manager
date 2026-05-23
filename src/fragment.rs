/// Windows Terminal JSON Fragment generator.
///
/// Writes profiles.json to:
///   %LOCALAPPDATA%\Microsoft\Windows Terminal\Fragments\wt-ssh-manager\
///
/// UUID generation uses the official WT namespace spec:
///   https://learn.microsoft.com/en-us/windows/terminal/json-fragment-extensions
use anyhow::Result;
use serde_json::{json, Value};
use std::path::PathBuf;
use uuid::Uuid;

use crate::config::{ConfigManager, ServerConfig};

const WT_NAMESPACE: Uuid = uuid::uuid!("f65ddb7e-706b-4499-8a50-40313caf510a");
const APP_NAME: &str = "wt-ssh-manager";

fn fragments_dir() -> PathBuf {
    let local = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs::home_dir().unwrap_or_default().join("AppData").join("Local"));
    local.join("Microsoft").join("Windows Terminal").join("Fragments").join(APP_NAME)
}

/// Compute the WT namespace UUID for this application (matches Python implementation).
fn app_namespace() -> Uuid {
    // WT spec: encode name as UTF-16LE bytes, then pass raw bytes to UUID v5
    let name_bytes: Vec<u8> =
        APP_NAME.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    Uuid::new_v5(&WT_NAMESPACE, &name_bytes)
}

fn profile_guid(server_name: &str) -> String {
    let app_ns = app_namespace();
    let name_bytes: Vec<u8> =
        server_name.encode_utf16().flat_map(|c| c.to_le_bytes()).collect();
    let guid = Uuid::new_v5(&app_ns, &name_bytes);
    format!("{{{guid}}}")
}

fn exe_path() -> String {
    std::env::current_exe()
        .unwrap_or_else(|_| PathBuf::from("ssh-manager"))
        .to_string_lossy()
        .into_owned()
}

fn build_profile(server: &ServerConfig) -> Value {
    let guid = profile_guid(&server.name);
    let exe = exe_path();
    let commandline = format!("\"{}\" connect {}", exe, server.id);
    json!({
        "guid":           guid,
        "name":           format!("\u{1f5a5}  {}", server.name),
        "commandline":    commandline,
        "tabTitle":       server.name,
        "colorScheme":    server.color,
        "startingDirectory": "%USERPROFILE%",
        "icon":           "\u{1f5a5}"
    })
}

/// Write the Fragment JSON and return (path, profile count).
pub fn sync_fragment(cfg: &ConfigManager) -> Result<(PathBuf, usize)> {
    let dir = fragments_dir();
    std::fs::create_dir_all(&dir)?;

    let profiles: Vec<Value> = cfg.list_servers().iter().map(build_profile).collect();
    let count = profiles.len();
    let fragment = json!({ "profiles": profiles });

    let path = dir.join("profiles.json");
    std::fs::write(&path, serde_json::to_string_pretty(&fragment)?)?;
    Ok((path, count))
}
