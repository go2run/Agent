//! Auto-detect the best available storage backend.
//!
//! Priority: IndexedDB → Memory (fallback)
//! OPFS support will be added when Worker context is available.

use std::rc::Rc;
use agent_core::ports::StoragePort;
use agent_types::Result;
use super::{IndexedDbStorage, MemoryStorage};

/// Try to open the best available storage backend.
/// Returns a trait object so callers are backend-agnostic.
pub async fn auto_detect_storage() -> Result<Rc<dyn StoragePort>> {
    // Try IndexedDB first (persistent)
    match IndexedDbStorage::open().await {
        Ok(idb) => {
            log::info!("Storage backend: IndexedDB");
            Ok(Rc::new(idb))
        }
        Err(e) => {
            log::warn!("IndexedDB unavailable ({}), falling back to memory", e);
            Ok(Rc::new(MemoryStorage::new()))
        }
    }
}
