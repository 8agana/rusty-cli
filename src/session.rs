use crate::providers::ChatMessage;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SessionFile {
    pub messages: Vec<ChatMessage>,
}

pub struct SessionStore;

impl SessionStore {
    pub fn dir() -> Result<PathBuf> {
        let base = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("cannot resolve data dir"))?;
        Ok(base.join("rusty-cli").join("sessions"))
    }

    pub fn path(session: &str) -> Result<PathBuf> {
        Ok(Self::dir()?.join(format!("{}.json", session)))
    }

    pub fn load(session: &str) -> Result<Vec<ChatMessage>> {
        let path = Self::path(session)?;
        if !path.exists() {
            return Ok(vec![]);
        }
        let text =
            fs::read_to_string(&path).with_context(|| format!("reading session {}", session))?;
        let file: SessionFile =
            serde_json::from_str(&text).with_context(|| "parsing session json")?;
        Ok(file.messages)
    }

    pub fn save(session: &str, messages: &[ChatMessage]) -> Result<()> {
        let path = Self::path(session)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(&SessionFile {
            messages: messages.to_vec(),
        })?;
        fs::write(&path, data).with_context(|| format!("writing session {}", session))?;
        Ok(())
    }

    pub fn list() -> Result<Vec<String>> {
        let dir = Self::dir()?;
        let mut out = vec![];
        if dir.exists() {
            for entry in std::fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.extension().and_then(|s| s.to_str()) == Some("json")
                    && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                {
                    out.push(stem.to_string());
                }
            }
        }
        out.sort();
        Ok(out)
    }

    pub fn delete(session: &str) -> Result<()> {
        let path = Self::path(session)?;
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    pub fn clear_all() -> Result<()> {
        let dir = Self::dir()?;
        if dir.exists() {
            for entry in std::fs::read_dir(dir)? {
                let e = entry?;
                let _ = std::fs::remove_file(e.path());
            }
        }
        Ok(())
    }
}
