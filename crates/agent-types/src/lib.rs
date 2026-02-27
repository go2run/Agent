pub mod message;
pub mod event;
pub mod tool;
pub mod config;
pub mod error;
pub mod session;

#[cfg(test)]
mod tests;

pub use error::AgentError;
pub type Result<T> = std::result::Result<T, AgentError>;
