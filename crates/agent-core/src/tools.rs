//! Built-in tool definitions and tool registry.
//!
//! Tools follow the OpenAI function-calling schema so they work across providers.

use std::collections::HashMap;
use agent_types::tool::{ToolDefinition, ToolParameters};
use serde_json::{json, Map, Value};

/// Registry of available tools
pub struct ToolRegistry {
    tools: HashMap<String, ToolDefinition>,
}

impl ToolRegistry {
    pub fn new() -> Self {
        let mut registry = Self {
            tools: HashMap::new(),
        };
        registry.register_builtins();
        registry
    }

    pub fn get(&self, name: &str) -> Option<&ToolDefinition> {
        self.tools.get(name)
    }

    pub fn definitions(&self) -> Vec<ToolDefinition> {
        self.tools.values().cloned().collect()
    }

    fn register(&mut self, tool: ToolDefinition) {
        self.tools.insert(tool.name.clone(), tool);
    }

    fn register_builtins(&mut self) {
        self.register(Self::bash_tool());
        self.register(Self::read_file_tool());
        self.register(Self::write_file_tool());
        self.register(Self::list_dir_tool());
    }

    fn bash_tool() -> ToolDefinition {
        let mut props = Map::new();
        props.insert("command".to_string(), json!({
            "type": "string",
            "description": "The bash command to execute"
        }));
        props.insert("timeout_ms".to_string(), json!({
            "type": "integer",
            "description": "Optional timeout in milliseconds"
        }));

        ToolDefinition {
            name: "bash".to_string(),
            description: "Execute a bash command in the WASIX shell environment".to_string(),
            parameters: ToolParameters {
                schema_type: "object".to_string(),
                properties: props,
                required: vec!["command".to_string()],
            },
        }
    }

    fn read_file_tool() -> ToolDefinition {
        let mut props = Map::new();
        props.insert("path".to_string(), json!({
            "type": "string",
            "description": "Path to the file to read"
        }));

        ToolDefinition {
            name: "read_file".to_string(),
            description: "Read the contents of a file from the virtual filesystem".to_string(),
            parameters: ToolParameters {
                schema_type: "object".to_string(),
                properties: props,
                required: vec!["path".to_string()],
            },
        }
    }

    fn write_file_tool() -> ToolDefinition {
        let mut props = Map::new();
        props.insert("path".to_string(), json!({
            "type": "string",
            "description": "Path to the file to write"
        }));
        props.insert("content".to_string(), json!({
            "type": "string",
            "description": "Content to write to the file"
        }));

        ToolDefinition {
            name: "write_file".to_string(),
            description: "Write content to a file in the virtual filesystem".to_string(),
            parameters: ToolParameters {
                schema_type: "object".to_string(),
                properties: props,
                required: vec!["path".to_string(), "content".to_string()],
            },
        }
    }

    fn list_dir_tool() -> ToolDefinition {
        let mut props = Map::new();
        props.insert("path".to_string(), json!({
            "type": "string",
            "description": "Directory path to list"
        }));

        ToolDefinition {
            name: "list_dir".to_string(),
            description: "List files and directories at the given path".to_string(),
            parameters: ToolParameters {
                schema_type: "object".to_string(),
                properties: props,
                required: vec!["path".to_string()],
            },
        }
    }
}

impl Default for ToolRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Parse a JSON arguments string into a serde_json::Value
pub fn parse_tool_args(args: &str) -> Result<Value, serde_json::Error> {
    serde_json::from_str(args)
}
