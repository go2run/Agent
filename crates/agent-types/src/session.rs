use serde::{Deserialize, Serialize};
use crate::message::Message;
use crate::config::AgentConfig;

/// A persisted conversation session
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: String,
    pub title: String,
    pub messages: Vec<Message>,
    pub created_at: String,
    pub updated_at: String,
    pub config: AgentConfig,
}

impl Session {
    pub fn new(id: String) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            id,
            title: "New Session".to_string(),
            messages: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
            config: AgentConfig::default(),
        }
    }
}

/// Summary of a session for listing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: String,
    pub title: String,
    pub updated_at: String,
    pub message_count: usize,
}
