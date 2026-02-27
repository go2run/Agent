//! WASM-target tests for agent-platform (Node.js runtime).
//!
//! Tests MemoryStorage and StorageVfs under wasm32-unknown-unknown
//! via `wasm-pack test --node`.
//!
//! IndexedDB tests require a browser and live in browser.rs.

use wasm_bindgen_test::*;

use agent_platform::storage::MemoryStorage;
use agent_platform::vfs::StorageVfs;
use agent_core::ports::{StoragePort, VfsPort};
use std::rc::Rc;

// ─── MemoryStorage Tests ─────────────────────────────────

#[wasm_bindgen_test]
fn memory_storage_backend_name() {
    let storage = MemoryStorage::new();
    assert_eq!(storage.backend_name(), "memory");
}

#[wasm_bindgen_test]
async fn memory_storage_get_missing() {
    let storage = MemoryStorage::new();
    let result = storage.get("nonexistent").await.unwrap();
    assert!(result.is_none());
}

#[wasm_bindgen_test]
async fn memory_storage_set_and_get() {
    let storage = MemoryStorage::new();
    storage.set("key1", b"value1").await.unwrap();
    let result = storage.get("key1").await.unwrap();
    assert_eq!(result, Some(b"value1".to_vec()));
}

#[wasm_bindgen_test]
async fn memory_storage_overwrite() {
    let storage = MemoryStorage::new();
    storage.set("key", b"v1").await.unwrap();
    storage.set("key", b"v2").await.unwrap();
    let result = storage.get("key").await.unwrap();
    assert_eq!(result, Some(b"v2".to_vec()));
}

#[wasm_bindgen_test]
async fn memory_storage_delete() {
    let storage = MemoryStorage::new();
    storage.set("key", b"val").await.unwrap();
    storage.delete("key").await.unwrap();
    let result = storage.get("key").await.unwrap();
    assert!(result.is_none());
}

#[wasm_bindgen_test]
async fn memory_storage_delete_nonexistent() {
    let storage = MemoryStorage::new();
    storage.delete("nonexistent").await.unwrap();
}

#[wasm_bindgen_test]
async fn memory_storage_list_keys() {
    let storage = MemoryStorage::new();
    storage.set("prefix:a", b"1").await.unwrap();
    storage.set("prefix:b", b"2").await.unwrap();
    storage.set("other:c", b"3").await.unwrap();

    let mut keys = storage.list_keys("prefix:").await.unwrap();
    keys.sort();
    assert_eq!(keys, vec!["prefix:a", "prefix:b"]);
}

#[wasm_bindgen_test]
async fn memory_storage_list_keys_empty_prefix() {
    let storage = MemoryStorage::new();
    storage.set("a", b"1").await.unwrap();
    storage.set("b", b"2").await.unwrap();
    let keys = storage.list_keys("").await.unwrap();
    assert_eq!(keys.len(), 2);
}

#[wasm_bindgen_test]
async fn memory_storage_list_keys_no_match() {
    let storage = MemoryStorage::new();
    storage.set("key1", b"val").await.unwrap();
    let keys = storage.list_keys("nomatch:").await.unwrap();
    assert!(keys.is_empty());
}

#[wasm_bindgen_test]
async fn memory_storage_exists() {
    let storage = MemoryStorage::new();
    assert!(!storage.exists("key").await.unwrap());
    storage.set("key", b"val").await.unwrap();
    assert!(storage.exists("key").await.unwrap());
}

#[wasm_bindgen_test]
async fn memory_storage_binary_data() {
    let storage = MemoryStorage::new();
    let binary = vec![0u8, 1, 2, 255, 254, 253];
    storage.set("bin", &binary).await.unwrap();
    let result = storage.get("bin").await.unwrap().unwrap();
    assert_eq!(result, binary);
}

#[wasm_bindgen_test]
async fn memory_storage_empty_value() {
    let storage = MemoryStorage::new();
    storage.set("empty", b"").await.unwrap();
    let result = storage.get("empty").await.unwrap().unwrap();
    assert!(result.is_empty());
}

#[wasm_bindgen_test]
async fn memory_storage_large_data() {
    let storage = MemoryStorage::new();
    let large = vec![42u8; 100_000]; // 100KB (smaller than native for WASM)
    storage.set("large", &large).await.unwrap();
    let result = storage.get("large").await.unwrap().unwrap();
    assert_eq!(result.len(), 100_000);
}

// ─── VFS Tests (built on MemoryStorage) ──────────────────

fn make_vfs() -> StorageVfs {
    StorageVfs::new(Rc::new(MemoryStorage::new()))
}

#[wasm_bindgen_test]
async fn vfs_write_and_read() {
    let vfs = make_vfs();
    vfs.write_file("/hello.txt", b"Hello, World!").await.unwrap();
    let data = vfs.read_file("/hello.txt").await.unwrap();
    assert_eq!(data, b"Hello, World!");
}

#[wasm_bindgen_test]
async fn vfs_read_nonexistent() {
    let vfs = make_vfs();
    let result = vfs.read_file("/nonexistent.txt").await;
    assert!(result.is_err());
}

#[wasm_bindgen_test]
async fn vfs_overwrite() {
    let vfs = make_vfs();
    vfs.write_file("/file.txt", b"first").await.unwrap();
    vfs.write_file("/file.txt", b"second").await.unwrap();
    let data = vfs.read_file("/file.txt").await.unwrap();
    assert_eq!(data, b"second");
}

#[wasm_bindgen_test]
async fn vfs_delete() {
    let vfs = make_vfs();
    vfs.write_file("/del.txt", b"data").await.unwrap();
    vfs.delete_file("/del.txt").await.unwrap();
    let result = vfs.read_file("/del.txt").await;
    assert!(result.is_err());
}

#[wasm_bindgen_test]
async fn vfs_exists() {
    let vfs = make_vfs();
    assert!(!vfs.exists("/file.txt").await.unwrap());
    vfs.write_file("/file.txt", b"data").await.unwrap();
    assert!(vfs.exists("/file.txt").await.unwrap());
}

#[wasm_bindgen_test]
async fn vfs_mkdir() {
    let vfs = make_vfs();
    vfs.mkdir("/mydir").await.unwrap();
    let stat = vfs.stat("/mydir").await.unwrap();
    assert!(stat.is_dir);
}

#[wasm_bindgen_test]
async fn vfs_stat_file() {
    let vfs = make_vfs();
    vfs.write_file("/data.bin", b"12345").await.unwrap();
    let stat = vfs.stat("/data.bin").await.unwrap();
    assert!(!stat.is_dir);
    assert_eq!(stat.size, 5);
}

#[wasm_bindgen_test]
async fn vfs_stat_nonexistent() {
    let vfs = make_vfs();
    let result = vfs.stat("/nowhere").await;
    assert!(result.is_err());
}

#[wasm_bindgen_test]
async fn vfs_list_dir() {
    let vfs = make_vfs();
    vfs.write_file("/dir/a.txt", b"a").await.unwrap();
    vfs.write_file("/dir/b.txt", b"b").await.unwrap();

    let entries = vfs.list_dir("/dir").await.unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"a.txt"), "Missing a.txt in {:?}", names);
    assert!(names.contains(&"b.txt"), "Missing b.txt in {:?}", names);
}

#[wasm_bindgen_test]
async fn vfs_list_dir_empty() {
    let vfs = make_vfs();
    vfs.mkdir("/empty").await.unwrap();
    let entries = vfs.list_dir("/empty").await.unwrap();
    assert!(entries.is_empty());
}

#[wasm_bindgen_test]
async fn vfs_nested_dirs() {
    let vfs = make_vfs();
    vfs.write_file("/a/b/c/file.txt", b"deep").await.unwrap();
    let data = vfs.read_file("/a/b/c/file.txt").await.unwrap();
    assert_eq!(data, b"deep");
}

#[wasm_bindgen_test]
async fn vfs_path_normalization() {
    let vfs = make_vfs();
    vfs.write_file("relative.txt", b"rel").await.unwrap();
    let data = vfs.read_file("/relative.txt").await.unwrap();
    assert_eq!(data, b"rel");
}

#[wasm_bindgen_test]
async fn vfs_binary_file() {
    let vfs = make_vfs();
    let binary = (0..256).map(|i| i as u8).collect::<Vec<u8>>();
    vfs.write_file("/bin.dat", &binary).await.unwrap();
    let data = vfs.read_file("/bin.dat").await.unwrap();
    assert_eq!(data, binary);
}

#[wasm_bindgen_test]
async fn vfs_empty_file() {
    let vfs = make_vfs();
    vfs.write_file("/empty.txt", b"").await.unwrap();
    let data = vfs.read_file("/empty.txt").await.unwrap();
    assert!(data.is_empty());
    let stat = vfs.stat("/empty.txt").await.unwrap();
    assert_eq!(stat.size, 0);
}

#[wasm_bindgen_test]
async fn vfs_unicode_content() {
    let vfs = make_vfs();
    let text = "你好世界 🌍 こんにちは";
    vfs.write_file("/unicode.txt", text.as_bytes()).await.unwrap();
    let data = vfs.read_file("/unicode.txt").await.unwrap();
    assert_eq!(String::from_utf8(data).unwrap(), text);
}

#[wasm_bindgen_test]
async fn vfs_unicode_filename() {
    let vfs = make_vfs();
    vfs.write_file("/文件.txt", b"content").await.unwrap();
    let data = vfs.read_file("/文件.txt").await.unwrap();
    assert_eq!(data, b"content");
}
