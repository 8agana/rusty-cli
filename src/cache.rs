use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntry<T> {
    pub value: T,
}

pub struct CacheStore;

impl CacheStore {
    pub fn dir() -> Result<PathBuf> {
        let base = dirs::data_dir().ok_or_else(|| anyhow::anyhow!("cannot resolve data dir"))?;
        Ok(base.join("rusty-cli").join("cache"))
    }

    fn path_for_key(key: &str) -> Result<PathBuf> {
        Ok(Self::dir()?.join(format!("{}.json", key)))
    }

    pub fn get<T: for<'de> Deserialize<'de>>(key: &str) -> Result<Option<T>> {
        let path = Self::path_for_key(key)?;
        if !path.exists() {
            return Ok(None);
        }
        let text = fs::read_to_string(&path)?;
        let entry: CacheEntry<T> = serde_json::from_str(&text)?;
        Ok(Some(entry.value))
    }

    pub fn put<T: Serialize>(key: &str, value: T) -> Result<()> {
        let path = Self::path_for_key(key)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let entry = CacheEntry { value };
        let text = serde_json::to_string_pretty(&entry)?;
        fs::write(&path, text)?;
        Ok(())
    }
}

#[allow(dead_code)]
pub fn hash_bytes(bytes: &[u8]) -> String {
    let h = blake3::hash(bytes);
    h.to_hex().to_string()
}
