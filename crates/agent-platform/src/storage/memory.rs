//! In-memory storage backend.
//! Fastest option but not persistent across page reloads.

use std::cell::RefCell;
use std::collections::HashMap;
use async_trait::async_trait;
use agent_core::ports::StoragePort;
use agent_types::Result;

pub struct MemoryStorage {
    data: RefCell<HashMap<String, Vec<u8>>>,
}

impl MemoryStorage {
    pub fn new() -> Self {
        Self {
            data: RefCell::new(HashMap::new()),
        }
    }
}

#[async_trait(?Send)]
impl StoragePort for MemoryStorage {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.data.borrow().get(key).cloned())
    }

    async fn set(&self, key: &str, value: &[u8]) -> Result<()> {
        self.data
            .borrow_mut()
            .insert(key.to_string(), value.to_vec());
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        self.data.borrow_mut().remove(key);
        Ok(())
    }

    async fn list_keys(&self, prefix: &str) -> Result<Vec<String>> {
        let keys: Vec<String> = self
            .data
            .borrow()
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        Ok(keys)
    }

    fn backend_name(&self) -> &str {
        "memory"
    }
}
