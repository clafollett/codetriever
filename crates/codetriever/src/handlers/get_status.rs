//! Auto-generated handler for `/get_status` endpoint.

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

/// Auto-generated unified parameters struct for `/get_status` endpoint.
/// Combines query parameters and request body properties into a single MCP interface.
/// Spec:
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct GetStatusParams {}

// Implement Endpoint for generic handler
impl Endpoint for GetStatusParams {
    fn path() -> &'static str {
        "/status"
    }

    fn get_params(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl GetStatusParams {}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct GetStatusResponse {
    #[schemars(description = r#" - "#)]
    pub index: Option<serde_json::Value>,
    #[schemars(description = r#" - "#)]
    pub performance: Option<serde_json::Value>,
    #[schemars(description = r#" - "#)]
    pub server: Option<serde_json::Value>,
    #[schemars(description = r#" - "#)]
    pub watcher: Option<serde_json::Value>,
}

impl IntoContents for GetStatusResponse {
    fn into_contents(self) -> Vec<Content> {
        // Convert the response into a Vec<Content> as expected by MCP
        // Panics only if serialization fails, which should be impossible for valid structs
        vec![Content::json(self).expect("Failed to serialize GetStatusResponse to Content")]
    }
}

/// `/status` endpoint handler
/// Check health, index jobs, and performance metrics
/// Use this to understand the current state of the codetriever system. Shows: active indexing jobs and their progress, file watcher status, index freshness, performance metrics, and any errors. Check this when searches seem slow or outdated, before starting large operations, or to monitor background indexing.
#[doc = r#"Verb: GET
Path: /status
Parameters: GetStatusParams
Responses:
    200: Successful Operation
    400: Bad input parameter
    500: Internal Server Error
    502: Bad Gateway
    503: Service Unavailable
    504: Gateway Timeout
Tag: untagged"#]
pub async fn get_status_handler(
    config: &Config,
    params: &GetStatusParams,
) -> Result<CallToolResult, agenterra_rmcp::Error> {
    // Log incoming request parameters and request details as structured JSON
    info!(
        target = "handler",
        event = "incoming_request",
        endpoint = "get_status",
        method = "GET",
        path = "/status",
        params = serde_json::to_string(params).unwrap_or_else(|e| {
            warn!("Failed to serialize request params: {e}");
            "{}".to_string()
        })
    );
    debug!(
        target = "handler",
        event = "before_api_call",
        endpoint = "get_status"
    );
    let request_body = None;
    let resp =
        get_endpoint_response::<_, GetStatusResponse>(config, params, "GET", request_body).await;

    match &resp {
        Ok(r) => {
            info!(
                target = "handler",
                event = "api_response",
                endpoint = "get_status",
                response = ?r
            );
        }
        Err(e) => {
            error!(target = "handler", event = "api_error", endpoint = "get_status", error = ?e);
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
        let params = GetStatusParams {};
        let _ = serde_json::to_string(&params).expect("Serializing test params should not fail");
    }
}
