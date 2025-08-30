//! Auto-generated handler for `/index` endpoint.

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

/// Auto-generated parameters struct for `/index` endpoint.
/// Spec: 
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct IndexParams {
}

// Implement Endpoint for generic handler
impl Endpoint for IndexParams {
    fn path() -> &'static str {
        "/index"
    }

    fn get_params(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

/// Auto-generated properties struct for `/index` endpoint.
/// Spec: 
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct IndexProperties {
#[schemars(description = r#" - "#)]
    pub mode: Option<String>,
    #[schemars(description = r#" - Return immediately (MCP) or wait (CLI)"#)]
    pub async_: Option<bool>,
    #[schemars(description = r#" - Specific paths to index"#)]
    pub paths: Option<Vec<String>>,
    #[schemars(description = r#" - Max wait time for async operations"#)]
    pub timeout_ms: Option<i32>,
    }
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct IndexResponse(pub serde_json::Value);

impl IntoContents for IndexResponse {
    fn into_contents(self) -> Vec<Content> {
        // Convert the response into a Vec<Content> as expected by MCP
        // Panics only if serialization fails, which should be impossible for valid structs
        vec![Content::json(self).expect("Failed to serialize IndexResponse to Content")]
    }
}

/// `/index` endpoint handler
/// Refresh the code index (usually automatic)
/// Triggers a reindex of the codebase. Usually runs automatically via file watcher, but use this when: switching branches and need immediate index update, after large refactoring, when status shows stale files, or to force a full rebuild. Returns immediately with job ID (async mode) or waits for completion (sync mode). Check progress via the /status endpoint.
#[doc = r#"Verb: GET
Path: /index
Parameters: IndexParams
Responses:
    200: Successful Operation
    400: Bad input parameter
    500: Internal Server Error
    502: Bad Gateway
    503: Service Unavailable
    504: Gateway Timeout
Tag: untagged"#]
pub async fn index_handler(
    config: &Config,
    params: &IndexParams,
) -> Result<CallToolResult, agenterra_rmcp::Error> {
    // Log incoming request parameters and request details as structured JSON
    info!(
        target = "handler",
        event = "incoming_request",
        endpoint = "index",
        method = "GET",
        path = "/index",
        params = serde_json::to_string(params).unwrap_or_else(|e| {
            warn!("Failed to serialize request params: {e}");
            "{}".to_string()
        })
    );
    debug!(
        target = "handler",
        event = "before_api_call",
        endpoint = "index"
    );
    let resp = get_endpoint_response::<_, IndexResponse>(config, params).await;

    match &resp {
        Ok(r) => {
            info!(
                target = "handler",
                event = "api_response",
                endpoint = "index",
                response = ?r
            );
        }
        Err(e) => {
            error!(target = "handler", event = "api_error", endpoint = "index", error = ?e);
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
        let params = IndexParams {
        };
        let _ = serde_json::to_string(&params).expect("Serializing test params should not fail");
    }

    #[test]
    fn test_properties_struct_serialization() {
        let props = IndexProperties {
        mode: None,
            async_: None,
            paths: None,
            timeout_ms: None,
            };
        let _ = serde_json::to_string(&props).expect("Serializing test properties should not fail");
    }
}
