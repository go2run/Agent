//! Virtual Filesystem — built on top of StoragePort.
//!
//! Maps POSIX-like paths to storage keys:
//!   /home/user/file.txt → "vfs:/home/user/file.txt"
//!
//! Directory structure is maintained via prefix-based key listing.

use std::rc::Rc;
use async_trait::async_trait;
use agent_core::ports::{StoragePort, VfsPort};
use agent_types::{
    AgentError, Result,
    tool::{DirEntry, FileStat},
};

const VFS_PREFIX: &str = "vfs:";
const DIR_MARKER: &str = "__dir__";

pub struct StorageVfs {
    storage: Rc<dyn StoragePort>,
}

impl StorageVfs {
    pub fn new(storage: Rc<dyn StoragePort>) -> Self {
        Self { storage }
    }

    fn key_for_path(&self, path: &str) -> String {
        let normalized = normalize_path(path);
        format!("{}{}", VFS_PREFIX, normalized)
    }

    fn dir_key(&self, path: &str) -> String {
        let normalized = normalize_path(path);
        format!("{}{}/{}", VFS_PREFIX, normalized, DIR_MARKER)
    }

    #[allow(dead_code)]
    fn path_from_key(&self, key: &str) -> String {
        key.strip_prefix(VFS_PREFIX).unwrap_or(key).to_string()
    }
}

#[async_trait(?Send)]
impl VfsPort for StorageVfs {
    async fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        let key = self.key_for_path(path);
        self.storage
            .get(&key)
            .await?
            .ok_or_else(|| AgentError::Fs {
                path: path.to_string(),
                message: "File not found".to_string(),
            })
    }

    async fn write_file(&self, path: &str, data: &[u8]) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = parent_path(path) {
            self.mkdir(&parent).await?;
        }
        let key = self.key_for_path(path);
        self.storage.set(&key, data).await
    }

    async fn delete_file(&self, path: &str) -> Result<()> {
        let key = self.key_for_path(path);
        self.storage.delete(&key).await
    }

    async fn list_dir(&self, path: &str) -> Result<Vec<DirEntry>> {
        let normalized = normalize_path(path);
        let prefix = format!("{}{}/", VFS_PREFIX, normalized);
        let keys = self.storage.list_keys(&prefix).await?;

        let mut entries = std::collections::HashMap::new();

        for key in &keys {
            let rel = key.strip_prefix(&prefix).unwrap_or(key);
            // Get the first path component after the prefix
            let name = if let Some(slash_pos) = rel.find('/') {
                &rel[..slash_pos]
            } else {
                rel
            };

            if name == DIR_MARKER || name.is_empty() {
                continue;
            }

            let is_dir = rel.contains('/');
            let size = if !is_dir {
                self.storage
                    .get(key)
                    .await?
                    .map(|v| v.len() as u64)
                    .unwrap_or(0)
            } else {
                0
            };

            entries.entry(name.to_string()).or_insert(DirEntry {
                name: name.to_string(),
                is_dir,
                size,
            });
        }

        let mut result: Vec<DirEntry> = entries.into_values().collect();
        result.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(result)
    }

    async fn stat(&self, path: &str) -> Result<FileStat> {
        let key = self.key_for_path(path);

        // Check if it's a file
        if let Some(data) = self.storage.get(&key).await? {
            return Ok(FileStat {
                size: data.len() as u64,
                is_dir: false,
                modified: None,
            });
        }

        // Check if it's a directory
        let dir_key = self.dir_key(path);
        if self.storage.exists(&dir_key).await? {
            return Ok(FileStat {
                size: 0,
                is_dir: true,
                modified: None,
            });
        }

        Err(AgentError::Fs {
            path: path.to_string(),
            message: "Not found".to_string(),
        })
    }

    async fn mkdir(&self, path: &str) -> Result<()> {
        let dir_key = self.dir_key(path);
        self.storage.set(&dir_key, b"").await
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let key = self.key_for_path(path);
        if self.storage.exists(&key).await? {
            return Ok(true);
        }
        let dir_key = self.dir_key(path);
        self.storage.exists(&dir_key).await
    }
}

/// Normalize a path: remove trailing slashes, ensure leading slash
fn normalize_path(path: &str) -> String {
    let path = path.trim();
    let path = if path.is_empty() || path == "/" {
        return String::new();
    } else {
        path
    };
    let path = if !path.starts_with('/') {
        format!("/{}", path)
    } else {
        path.to_string()
    };
    path.trim_end_matches('/').to_string()
}

/// Get the parent directory of a path
fn parent_path(path: &str) -> Option<String> {
    let normalized = normalize_path(path);
    if let Some(last_slash) = normalized.rfind('/') {
        if last_slash == 0 {
            Some("/".to_string())
        } else {
            Some(normalized[..last_slash].to_string())
        }
    } else {
        None
    }
}
