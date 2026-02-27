//! IndexedDB storage backend.
//! Persistent across page reloads. Works in all modern browsers.
//! Uses web-sys bindings with wasm-bindgen-futures for async operations.

use async_trait::async_trait;
use js_sys::{Array, Uint8Array};
use wasm_bindgen::prelude::*;
use wasm_bindgen_futures::JsFuture;
use web_sys::{IdbDatabase, IdbTransactionMode};

use agent_core::ports::StoragePort;
use agent_types::{AgentError, Result};

const DB_NAME: &str = "agent_storage";
const STORE_NAME: &str = "kv";
const DB_VERSION: u32 = 1;

pub struct IndexedDbStorage {
    db: IdbDatabase,
}

impl IndexedDbStorage {
    /// Open (or create) the IndexedDB database.
    pub async fn open() -> Result<Self> {
        let window = web_sys::window()
            .ok_or_else(|| AgentError::Storage("No window object".to_string()))?;

        let idb_factory = window
            .indexed_db()
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?
            .ok_or_else(|| AgentError::Storage("IndexedDB not available".to_string()))?;

        let open_req = idb_factory
            .open_with_u32(DB_NAME, DB_VERSION)
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?;

        // Handle upgrade: create object store if needed
        let open_req_clone = open_req.clone();
        let onupgrade = Closure::once(move |_event: web_sys::Event| {
            let db: IdbDatabase = open_req_clone
                .result()
                .unwrap()
                .dyn_into()
                .unwrap();
            // Try to create the object store; ignore error if it already exists
            let _ = db.create_object_store(STORE_NAME);
        });
        open_req.set_onupgradeneeded(Some(onupgrade.as_ref().unchecked_ref()));
        onupgrade.forget();

        let db: IdbDatabase = JsFuture::from(idb_request_to_promise(&open_req)?)
            .await
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?
            .dyn_into()
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?;

        Ok(Self { db })
    }

    fn transaction(&self, mode: IdbTransactionMode) -> Result<web_sys::IdbObjectStore> {
        let tx = self
            .db
            .transaction_with_str_and_mode(STORE_NAME, mode)
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?;
        tx.object_store(STORE_NAME)
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))
    }
}

#[async_trait(?Send)]
impl StoragePort for IndexedDbStorage {
    async fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let store = self.transaction(IdbTransactionMode::Readonly)?;
        let req = store
            .get(&JsValue::from_str(key))
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?;

        let result = JsFuture::from(idb_request_to_promise(&req)?)
            .await
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?;

        if result.is_undefined() || result.is_null() {
            return Ok(None);
        }

        let array = Uint8Array::new(&result);
        Ok(Some(array.to_vec()))
    }

    async fn set(&self, key: &str, value: &[u8]) -> Result<()> {
        let store = self.transaction(IdbTransactionMode::Readwrite)?;
        let js_value = Uint8Array::from(value);
        store
            .put_with_key(&js_value, &JsValue::from_str(key))
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?;
        Ok(())
    }

    async fn delete(&self, key: &str) -> Result<()> {
        let store = self.transaction(IdbTransactionMode::Readwrite)?;
        store
            .delete(&JsValue::from_str(key))
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?;
        Ok(())
    }

    async fn list_keys(&self, prefix: &str) -> Result<Vec<String>> {
        let store = self.transaction(IdbTransactionMode::Readonly)?;
        let req = store
            .get_all_keys()
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?;

        let result = JsFuture::from(idb_request_to_promise(&req)?)
            .await
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?;

        let array: Array = result
            .dyn_into()
            .map_err(|e| AgentError::Storage(format!("{:?}", e)))?;

        let mut keys = Vec::new();
        for i in 0..array.length() {
            if let Some(key) = array.get(i).as_string() {
                if key.starts_with(prefix) {
                    keys.push(key);
                }
            }
        }
        Ok(keys)
    }

    fn backend_name(&self) -> &str {
        "indexeddb"
    }
}

/// Convert an IdbRequest to a JS Promise for use with JsFuture.
/// Wraps the callback-based IDB API into a Future-compatible Promise.
fn idb_request_to_promise(req: &web_sys::IdbRequest) -> Result<js_sys::Promise> {
    let req_for_success = req.clone();
    let req_for_callbacks = req.clone();

    let promise = js_sys::Promise::new(&mut move |resolve, reject| {
        let req_inner = req_for_success.clone();
        let onsuccess = Closure::once(move |_: web_sys::Event| {
            let _ = resolve.call1(
                &JsValue::NULL,
                &req_inner.result().unwrap_or(JsValue::UNDEFINED),
            );
        });
        let onerror = Closure::once(move |_: web_sys::Event| {
            let _ = reject.call1(&JsValue::NULL, &JsValue::from_str("IDB request failed"));
        });
        req_for_callbacks.set_onsuccess(Some(onsuccess.as_ref().unchecked_ref()));
        req_for_callbacks.set_onerror(Some(onerror.as_ref().unchecked_ref()));
        onsuccess.forget();
        onerror.forget();
    });
    Ok(promise)
}
