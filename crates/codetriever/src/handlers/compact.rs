//! Auto-generated handler for `/compact` endpoint.

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

/// Auto-generated unified parameters struct for `/compact` endpoint.
/// Combines query parameters and request body properties into a single MCP interface.
/// Spec:
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct CompactParams {}

// Implement Endpoint for generic handler
impl Endpoint for CompactParams {
    fn path() -> &'static str {
        "/compact"
    }

    fn get_params(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl CompactParams {}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct CompactResponse {
    #[schemars(description = r#" - "#)]
    pub after_size_mb: Option<f64>,
    #[schemars(description = r#" - "#)]
    pub before_size_mb: Option<f64>,
    #[schemars(description = r#" - "#)]
    pub duration_ms: Option<i32>,
}

impl IntoContents for CompactResponse {
    fn into_contents(self) -> Vec<Content> {
        // Convert the response into a Vec<Content> as expected by MCP
        // Panics only if serialization fails, which should be impossible for valid structs
        vec![Content::json(self).expect("Failed to serialize CompactResponse to Content")]
    }
}

/// `/compact` endpoint handler
/// Optimize database for better performance
/// Runs database optimization to improve query speed and reduce file size. Similar to SQL Server index rebuild or PostgreSQL VACUUM. Use monthly or when /status shows degraded search performance. Safe to run anytime but may temporarily slow searches during compaction.
#[doc = r#"Verb: GET
Path: /compact
Parameters: CompactParams
Responses:
    200: Successful Operation
    400: Bad input parameter
    500: Internal Server Error
    502: Bad Gateway
    503: Service Unavailable
    504: Gateway Timeout
Tag: untagged"#]
pub async fn compact_handler(
    config: &Config,
    params: &CompactParams,
) -> Result<CallToolResult, agenterra_rmcp::Error> {
    // Log incoming request parameters and request details as structured JSON
    info!(
        target = "handler",
        event = "incoming_request",
        endpoint = "compact",
        method = "POST",
        path = "/compact",
        params = serde_json::to_string(params).unwrap_or_else(|e| {
            warn!("Failed to serialize request params: {e}");
            "{}".to_string()
        })
    );
    debug!(
        target = "handler",
        event = "before_api_call",
        endpoint = "compact"
    );
    let request_body = None;
    let resp =
        get_endpoint_response::<_, CompactResponse>(config, params, "POST", request_body).await;

    match &resp {
        Ok(r) => {
            info!(
                target = "handler",
                event = "api_response",
                endpoint = "compact",
                response = ?r
            );
        }
        Err(e) => {
            error!(target = "handler", event = "api_error", endpoint = "compact", error = ?e);
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
        let params = CompactParams {};
        let _ = serde_json::to_string(&params).expect("Serializing test params should not fail");
    }
}
