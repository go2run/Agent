pub mod memory;
pub mod indexeddb;
pub mod auto;

pub use memory::MemoryStorage;
pub use indexeddb::IndexedDbStorage;
pub use auto::auto_detect_storage;
