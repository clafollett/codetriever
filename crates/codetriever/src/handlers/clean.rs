//! Auto-generated handler for `/clean` endpoint.

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

/// Auto-generated unified parameters struct for `/clean` endpoint.
/// Combines query parameters and request body properties into a single MCP interface.
/// Spec:
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct CleanParams {
    #[schemars(description = r#"Duration string (e.g., \"7d\", \"1h\") (request body)"#)]
    pub older_than: Option<String>,
    #[schemars(description = r#"Request body property"#)]
    pub missing_files: Option<bool>,
}

// Implement Endpoint for generic handler
impl Endpoint for CleanParams {
    fn path() -> &'static str {
        "/clean"
    }

    fn get_params(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl CleanParams {
    /// Extract request body properties for REST API calls
    pub fn to_request_body(&self) -> CleanRequestBody {
        CleanRequestBody {
            older_than: self.older_than.clone(),
            missing_files: self.missing_files,
        }
    }
}

/// Request body structure for REST API calls
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CleanRequestBody {
    pub older_than: Option<String>,
    pub missing_files: Option<bool>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct CleanResponse {
    #[schemars(description = r#" - "#)]
    pub freed_space_mb: Option<f64>,
    #[schemars(description = r#" - "#)]
    pub removed_chunks: Option<i32>,
}

impl IntoContents for CleanResponse {
    fn into_contents(self) -> Vec<Content> {
        // Convert the response into a Vec<Content> as expected by MCP
        // Panics only if serialization fails, which should be impossible for valid structs
        vec![Content::json(self).expect("Failed to serialize CleanResponse to Content")]
    }
}

/// `/clean` endpoint handler
/// Clean up outdated index entries
/// Maintenance operation to remove stale data. Use when: index size grows too large, after deleting many files, to remove entries older than X days, or when switching between branches frequently. Frees up disk space and improves search performance.
#[doc = r#"Verb: GET
Path: /clean
Parameters: CleanParams
Responses:
    200: Successful Operation
    400: Bad input parameter
    500: Internal Server Error
    502: Bad Gateway
    503: Service Unavailable
    504: Gateway Timeout
Tag: untagged"#]
pub async fn clean_handler(
    config: &Config,
    params: &CleanParams,
) -> Result<CallToolResult, agenterra_rmcp::Error> {
    // Log incoming request parameters and request details as structured JSON
    info!(
        target = "handler",
        event = "incoming_request",
        endpoint = "clean",
        method = "POST",
        path = "/clean",
        params = serde_json::to_string(params).unwrap_or_else(|e| {
            warn!("Failed to serialize request params: {e}");
            "{}".to_string()
        })
    );
    debug!(
        target = "handler",
        event = "before_api_call",
        endpoint = "clean"
    );
    let request_body = serde_json::to_value(params.to_request_body()).ok();
    let resp =
        get_endpoint_response::<_, CleanResponse>(config, params, "POST", request_body).await;

    match &resp {
        Ok(r) => {
            info!(
                target = "handler",
                event = "api_response",
                endpoint = "clean",
                response = ?r
            );
        }
        Err(e) => {
            error!(target = "handler", event = "api_error", endpoint = "clean", error = ?e);
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
        let params = CleanParams {
            older_than: None,
            missing_files: None,
        };
        let _ = serde_json::to_string(&params).expect("Serializing test params should not fail");
    }
}
