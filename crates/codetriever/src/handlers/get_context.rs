//! Auto-generated handler for `/get_context` endpoint.

// Internal imports (std, crate)
use crate::common::*;
use crate::config::Config;

// External imports (alphabetized)
use agenterra_rmcp::handler::server::tool::IntoCallToolResult;
use agenterra_rmcp::model::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};
use utoipa::ToSchema;

/// Auto-generated unified parameters struct for `/get_context` endpoint.
/// Combines query parameters and request body properties into a single MCP interface.
/// Spec:
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct GetContextParams {
    #[schemars(description = r#"Request body property"#)]
    pub file: Option<String>,
    #[schemars(description = r#"Request body property"#)]
    pub line: Option<i32>,
    #[schemars(description = r#"Lines before and after (request body)"#)]
    pub radius: Option<i32>,
}

// Implement Endpoint for generic handler
impl Endpoint for GetContextParams {
    fn path() -> &'static str {
        "/context"
    }

    fn get_params(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl GetContextParams {
    /// Extract request body properties for REST API calls
    pub fn to_request_body(&self) -> GetContextRequestBody {
        GetContextRequestBody {
            file: self.file.clone(),
            line: self.line,
            radius: self.radius,
        }
    }
}

/// Request body structure for REST API calls
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GetContextRequestBody {
    pub file: Option<String>,
    pub line: Option<i32>,
    pub radius: Option<i32>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct GetContextResponse {
    #[schemars(description = r#" - "#)]
    pub content: Option<String>,
    #[schemars(description = r#" - "#)]
    pub file: Option<String>,
    #[schemars(description = r#" - "#)]
    pub line_end: Option<i32>,
    #[schemars(description = r#" - "#)]
    pub line_start: Option<i32>,
    #[schemars(description = r#" - "#)]
    pub symbols: Option<Vec<String>>,
}

impl IntoContents for GetContextResponse {
    fn into_contents(self) -> Vec<Content> {
        // Convert the response into a Vec<Content> as expected by MCP
        // Panics only if serialization fails, which should be impossible for valid structs
        vec![Content::json(self).expect("Failed to serialize GetContextResponse to Content")]
    }
}

/// `/context` endpoint handler
/// Get surrounding code context for a specific location
/// Use this when you need to understand code in its full context. Given a file and line number, returns the surrounding code including function signatures, class definitions, imports, and nearby related code. Essential when you need to see the \"bigger picture\" around a specific piece of code.
#[doc = r#"Verb: GET
Path: /context
Parameters: GetContextParams
Responses:
    200: Successful Operation
    400: Bad input parameter
    500: Internal Server Error
    502: Bad Gateway
    503: Service Unavailable
    504: Gateway Timeout
Tag: untagged"#]
pub async fn get_context_handler(
    config: &Config,
    params: &GetContextParams,
) -> Result<CallToolResult, agenterra_rmcp::Error> {
    // Log incoming request parameters and request details as structured JSON
    info!(
        target = "handler",
        event = "incoming_request",
        endpoint = "get_context",
        method = "POST",
        path = "/context",
        params = serde_json::to_string(params).unwrap_or_else(|e| {
            warn!("Failed to serialize request params: {e}");
            "{}".to_string()
        })
    );
    debug!(
        target = "handler",
        event = "before_api_call",
        endpoint = "get_context"
    );
    let request_body = match serde_json::to_value(params.to_request_body()) {
        Ok(val) => Some(val),
        Err(e) => {
            error!(
                target = "handler",
                event = "serialization_error",
                endpoint = "get_context",
                error = ?e,
                "Failed to serialize request body"
            );
            return Err(agenterra_rmcp::Error::from(
                agenterra_rmcp::model::ErrorData::new(
                    agenterra_rmcp::model::ErrorCode::INVALID_PARAMS,
                    format!("Failed to serialize request body: {e}"),
                    None,
                ),
            ));
        }
    };
    let resp =
        get_endpoint_response::<_, GetContextResponse>(config, params, "POST", request_body).await;

    match &resp {
        Ok(r) => {
            info!(
                target = "handler",
                event = "api_response",
                endpoint = "get_context",
                response = ?r
            );
        }
        Err(e) => {
            error!(target = "handler", event = "api_error", endpoint = "get_context", error = ?e);
        }
    }

    // Log outgoing API request as structured JSON
    resp.and_then(|r| r.into_call_tool_result())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;
    #[test]
    fn test_parameters_struct_serialization() {
        let params = GetContextParams {
            file: None,
            line: None,
            radius: None,
        };
        let _ = serde_json::to_string(&params).expect("Serializing test params should not fail");
    }
}
