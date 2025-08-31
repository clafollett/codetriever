//! Auto-generated handler for `/get_stats` endpoint.

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

/// Auto-generated parameters struct for `/get_stats` endpoint.
/// Spec:
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct GetStatsParams {}

// Implement Endpoint for generic handler
impl Endpoint for GetStatsParams {
    fn path() -> &'static str {
        "/stats"
    }

    fn get_params(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

/// Auto-generated properties struct for `/get_stats` endpoint.
/// Spec:
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct GetStatsProperties {}
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct GetStatsResponse {
    #[schemars(description = r#" - "#)]
    pub chunks: Option<i32>,
    #[schemars(description = r#" - "#)]
    pub db_size_mb: Option<f64>,
    #[schemars(description = r#" - "#)]
    pub files: Option<i32>,
    #[schemars(description = r#" - "#)]
    pub vectors: Option<i32>,
    #[schemars(description = r#" - "#)]
    pub last_indexed: Option<String>,
}

impl IntoContents for GetStatsResponse {
    fn into_contents(self) -> Vec<Content> {
        // Convert the response into a Vec<Content> as expected by MCP
        // Panics only if serialization fails, which should be impossible for valid structs
        vec![Content::json(self).expect("Failed to serialize GetStatsResponse to Content")]
    }
}

/// `/stats` endpoint handler
/// Get quick index statistics
/// Lightweight endpoint for basic metrics. Use when you just need numbers: total files indexed, chunk count, database size, last update time. Faster than /status when you don't need detailed job information.
#[doc = r#"Verb: GET
Path: /stats
Parameters: GetStatsParams
Responses:
    200: Successful Operation
    400: Bad input parameter
    500: Internal Server Error
    502: Bad Gateway
    503: Service Unavailable
    504: Gateway Timeout
Tag: untagged"#]
pub async fn get_stats_handler(
    config: &Config,
    params: &GetStatsParams,
) -> Result<CallToolResult, agenterra_rmcp::Error> {
    // Log incoming request parameters and request details as structured JSON
    info!(
        target = "handler",
        event = "incoming_request",
        endpoint = "get_stats",
        method = "GET",
        path = "/stats",
        params = serde_json::to_string(params).unwrap_or_else(|e| {
            warn!("Failed to serialize request params: {e}");
            "{}".to_string()
        })
    );
    debug!(
        target = "handler",
        event = "before_api_call",
        endpoint = "get_stats"
    );
    let resp = get_endpoint_response::<_, GetStatsResponse>(config, params).await;

    match &resp {
        Ok(r) => {
            info!(
                target = "handler",
                event = "api_response",
                endpoint = "get_stats",
                response = ?r
            );
        }
        Err(e) => {
            error!(target = "handler", event = "api_error", endpoint = "get_stats", error = ?e);
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
        let params = GetStatsParams {};
        let _ = serde_json::to_string(&params).expect("Serializing test params should not fail");
    }

    #[test]
    fn test_properties_struct_serialization() {
        let props = GetStatsProperties {};
        let _ = serde_json::to_string(&props).expect("Serializing test properties should not fail");
    }
}
