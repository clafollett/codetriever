//! Auto-generated handler for `/find_similar` endpoint.

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

/// Auto-generated parameters struct for `/find_similar` endpoint.
/// Spec: 
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct FindSimilarParams {
}

// Implement Endpoint for generic handler
impl Endpoint for FindSimilarParams {
    fn path() -> &'static str {
        "/similar"
    }

    fn get_params(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

/// Auto-generated properties struct for `/find_similar` endpoint.
/// Spec: 
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct FindSimilarProperties {
#[schemars(description = r#" - File to exclude from results"#)]
    pub exclude_file: Option<String>,
    #[schemars(description = r#" - "#)]
    pub limit: Option<i32>,
    #[schemars(description = r#" - Code snippet to find similar to"#)]
    pub code: Option<String>,
    }
#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct FindSimilarResponse {
    #[schemars(description = r#" - "#)]
    pub chunks: Option<Vec<serde_json::Value>>,
    #[schemars(description = r#" - "#)]
    pub query_time_ms: Option<i32>,
}

impl IntoContents for FindSimilarResponse {
    fn into_contents(self) -> Vec<Content> {
        // Convert the response into a Vec<Content> as expected by MCP
        // Panics only if serialization fails, which should be impossible for valid structs
        vec![Content::json(self).expect("Failed to serialize FindSimilarResponse to Content")]
    }
}

/// `/similar` endpoint handler
/// Find code similar to a given snippet
/// Use this when you have an example of code and want to find similar implementations. Useful for: finding duplicated logic that could be refactored, locating all error handling patterns similar to one you're reviewing, or discovering variations of the same algorithm. Returns code chunks ranked by similarity score.
#[doc = r#"Verb: GET
Path: /similar
Parameters: FindSimilarParams
Responses:
    200: Successful Operation
    400: Bad input parameter
    500: Internal Server Error
    502: Bad Gateway
    503: Service Unavailable
    504: Gateway Timeout
Tag: untagged"#]
pub async fn find_similar_handler(
    config: &Config,
    params: &FindSimilarParams,
) -> Result<CallToolResult, agenterra_rmcp::Error> {
    // Log incoming request parameters and request details as structured JSON
    info!(
        target = "handler",
        event = "incoming_request",
        endpoint = "find_similar",
        method = "GET",
        path = "/similar",
        params = serde_json::to_string(params).unwrap_or_else(|e| {
            warn!("Failed to serialize request params: {e}");
            "{}".to_string()
        })
    );
    debug!(
        target = "handler",
        event = "before_api_call",
        endpoint = "find_similar"
    );
    let resp = get_endpoint_response::<_, FindSimilarResponse>(config, params).await;

    match &resp {
        Ok(r) => {
            info!(
                target = "handler",
                event = "api_response",
                endpoint = "find_similar",
                response = ?r
            );
        }
        Err(e) => {
            error!(target = "handler", event = "api_error", endpoint = "find_similar", error = ?e);
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
        let params = FindSimilarParams {
        };
        let _ = serde_json::to_string(&params).expect("Serializing test params should not fail");
    }

    #[test]
    fn test_properties_struct_serialization() {
        let props = FindSimilarProperties {
        exclude_file: None,
            limit: None,
            code: None,
            };
        let _ = serde_json::to_string(&props).expect("Serializing test properties should not fail");
    }
}
