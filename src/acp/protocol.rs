use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest<T> {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    pub params: T,
}

impl<T> JsonRpcRequest<T> {
    pub fn new(id: u64, method: impl Into<String>, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse<T> {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<T>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl<T> JsonRpcResponse<T> {
    pub fn ok(id: u64, result: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, error: JsonRpcError) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification<T> {
    pub jsonrpc: String,
    pub method: String,
    pub params: T,
}

impl<T> JsonRpcNotification<T> {
    pub fn new(method: impl Into<String>, params: T) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientInfo {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    pub version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeParams {
    pub protocol_version: u32,
    pub client_capabilities: serde_json::Value,
    pub client_info: ClientInfo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InitializeResult {
    pub protocol_version: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_capabilities: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub agent_info: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewParams {
    pub cwd: String,
    pub mcp_servers: Vec<serde_json::Value>,
}

/// One selectable value for a session config option. Agents advertise model
/// choices through `session/new`, so pin can list them without knowing any
/// provider-specific model names.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfigChoice {
    pub value: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A configurable session setting such as the model or thought level.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfigOption {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    pub option_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<SessionConfigChoice>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionNewResult {
    pub session_id: String,
    #[serde(default)]
    pub config_options: Vec<SessionConfigOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSetConfigOptionParams {
    pub session_id: String,
    pub config_id: String,
    pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum ContentBlock {
    #[serde(rename = "text")]
    Text { text: String },
}

impl ContentBlock {
    pub fn text(text: impl Into<String>) -> Self {
        ContentBlock::Text { text: text.into() }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPromptParams {
    pub session_id: String,
    pub prompt: Vec<ContentBlock>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionPromptResult {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionCancelParams {
    pub session_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionUpdateParams {
    pub session_id: String,
    pub update: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionOption {
    pub option_id: String,
    pub name: String,
    pub kind: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionParams {
    pub session_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call: Option<serde_json::Value>,
    pub options: Vec<PermissionOption>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionOutcome {
    pub outcome: String,
    pub option_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestPermissionResult {
    pub outcome: RequestPermissionOutcome,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initialize_serialization() {
        let req = JsonRpcRequest::new(
            1,
            "initialize",
            InitializeParams {
                protocol_version: 1,
                client_capabilities: serde_json::json!({}),
                client_info: ClientInfo {
                    name: "pin".to_string(),
                    title: Some("Pin Issue Tracker".to_string()),
                    version: env!("PIN_VERSION").to_string(),
                },
            },
        );
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"protocolVersion\":1"));
        assert!(s.contains("\"clientInfo\""));
        assert!(s.contains("\"jsonrpc\":\"2.0\""));
    }

    #[test]
    fn test_request_permission_outcome_serialization() {
        let outcome = RequestPermissionResult {
            outcome: RequestPermissionOutcome {
                outcome: "selected".to_string(),
                option_id: "allow-once".to_string(),
            },
        };
        let resp = JsonRpcResponse::ok(5, outcome);
        let s = serde_json::to_string(&resp).unwrap();
        assert!(s.contains("\"optionId\":\"allow-once\""));
        assert!(s.contains("\"outcome\":\"selected\""));
    }

    #[test]
    fn test_session_prompt_serialization() {
        let req = JsonRpcRequest::new(
            2,
            "session/prompt",
            SessionPromptParams {
                session_id: "sess_123".to_string(),
                prompt: vec![ContentBlock::text("Hello")],
            },
        );
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"sessionId\":\"sess_123\""));
        assert!(s.contains("\"type\":\"text\""));
    }

    #[test]
    fn test_session_new_config_options_deserialization() {
        let json = serde_json::json!({
            "sessionId": "sess_123",
            "configOptions": [
                {
                    "id": "model",
                    "name": "Model",
                    "category": "model",
                    "type": "select",
                    "currentValue": "mock/default",
                    "options": [
                        { "value": "mock/default", "name": "Mock Default" },
                        { "value": "mock/fast", "name": "Mock Fast", "description": "Fast model" }
                    ]
                },
                {
                    "id": "mode",
                    "category": "mode",
                    "type": "select",
                    "currentValue": "default",
                    "options": [{ "value": "default", "name": "Default" }]
                }
            ]
        });

        let result: SessionNewResult = serde_json::from_value(json).unwrap();
        assert_eq!(result.session_id, "sess_123");
        assert_eq!(result.config_options.len(), 2);

        let model = result
            .config_options
            .iter()
            .find(|option| option.category.as_deref() == Some("model"))
            .unwrap();
        assert_eq!(model.id, "model");
        assert_eq!(model.current_value, Some(serde_json::json!("mock/default")));
        assert_eq!(model.options.len(), 2);
        assert_eq!(model.options[1].value, serde_json::json!("mock/fast"));
        assert_eq!(model.options[1].name.as_deref(), Some("Mock Fast"));
    }

    #[test]
    fn test_set_config_option_serialization() {
        let req = JsonRpcRequest::new(
            3,
            "session/set_config_option",
            SessionSetConfigOptionParams {
                session_id: "sess_123".to_string(),
                config_id: "model".to_string(),
                value: serde_json::json!("mock/fast"),
            },
        );
        let s = serde_json::to_string(&req).unwrap();
        assert!(s.contains("\"method\":\"session/set_config_option\""));
        assert!(s.contains("\"configId\":\"model\""));
        assert!(s.contains("\"value\":\"mock/fast\""));
    }
}
