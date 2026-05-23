/// Server configuration model and JSON persistence.
///
/// Config file: `~/.wt-ssh-manager/config.json`
/// Passwords are stored DPAPI-encrypted; plain text never touches disk.
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::crypto;

fn config_dir() -> PathBuf {
    dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")).join(".wt-ssh-manager")
}
fn config_file() -> PathBuf {
    config_dir().join("config.json")
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub encrypted_password: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "default_color")]
    pub color: String,
}

fn default_color() -> String {
    "One Half Dark".into()
}

#[derive(Serialize, Deserialize, Default)]
struct ConfigFile {
    servers: Vec<ServerConfig>,
}

pub struct ConfigManager {
    servers: Vec<ServerConfig>,
}

impl ConfigManager {
    pub fn load() -> Result<Self> {
        std::fs::create_dir_all(config_dir())?;
        let servers = if config_file().exists() {
            let raw = std::fs::read_to_string(config_file())?;
            serde_json::from_str::<ConfigFile>(&raw)?.servers
        } else {
            Vec::new()
        };
        Ok(Self { servers })
    }

    fn save(&self) -> Result<()> {
        let data = ConfigFile { servers: self.servers.clone() };
        std::fs::write(config_file(), serde_json::to_string_pretty(&data)?)?;
        Ok(())
    }

    pub fn list_servers(&self) -> &[ServerConfig] {
        &self.servers
    }

    pub fn get_server(&self, name_or_id: &str) -> Option<&ServerConfig> {
        self.servers.iter().find(|s| s.id == name_or_id || s.name == name_or_id)
    }

    fn get_mut(&mut self, name_or_id: &str) -> Option<&mut ServerConfig> {
        self.servers.iter_mut().find(|s| s.id == name_or_id || s.name == name_or_id)
    }

    pub fn add_server(
        &mut self,
        name: &str,
        host: &str,
        port: u16,
        username: &str,
        password: &str,
        description: &str,
    ) -> Result<ServerConfig> {
        let id = name.to_lowercase().replace(' ', "-");
        if self.get_server(&id).is_some() {
            anyhow::bail!("Server '{}' already exists", name);
        }
        let server = ServerConfig {
            id,
            name: name.to_string(),
            host: host.to_string(),
            port,
            username: username.to_string(),
            encrypted_password: crypto::encrypt_password(password)?,
            description: description.to_string(),
            tags: Vec::new(),
            color: default_color(),
        };
        self.servers.push(server.clone());
        self.save()?;
        Ok(server)
    }

    pub fn remove_server(&mut self, name_or_id: &str) -> Result<()> {
        let before = self.servers.len();
        self.servers.retain(|s| s.id != name_or_id && s.name != name_or_id);
        if self.servers.len() == before {
            anyhow::bail!("Server '{}' not found", name_or_id);
        }
        self.save()
    }

    pub fn update_server(
        &mut self,
        name_or_id: &str,
        host: Option<&str>,
        port: Option<u16>,
        username: Option<&str>,
        password: Option<&str>,
        description: Option<&str>,
    ) -> Result<()> {
        let s = self
            .get_mut(name_or_id)
            .ok_or_else(|| anyhow::anyhow!("Server '{}' not found", name_or_id))?;
        if let Some(v) = host        { s.host = v.into(); }
        if let Some(v) = port        { s.port = v; }
        if let Some(v) = username    { s.username = v.into(); }
        if let Some(v) = password    { s.encrypted_password = crypto::encrypt_password(v)?; }
        if let Some(v) = description { s.description = v.into(); }
        self.save()
    }

    pub fn get_password(&self, server: &ServerConfig) -> Result<String> {
        crypto::decrypt_password(&server.encrypted_password)
    }
}
