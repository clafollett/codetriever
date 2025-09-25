//! Auto-generated handler for `/search` endpoint.

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

/// Auto-generated unified parameters struct for `/search` endpoint.
/// Combines query parameters and request body properties into a single MCP interface.
/// Spec:
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct SearchParams {
    #[schemars(description = r#"Request body property"#)]
    pub limit: Option<i32>,
    #[schemars(description = r#"Natural language search query (request body)"#)]
    pub query: Option<String>,
    #[schemars(description = r#"Request body property"#)]
    pub threshold: Option<f64>,
}

// Implement Endpoint for generic handler
impl Endpoint for SearchParams {
    fn path() -> &'static str {
        "/search"
    }

    fn get_params(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl SearchParams {
    /// Extract request body properties for REST API calls
    pub fn to_request_body(&self) -> SearchRequestBody {
        SearchRequestBody {
            limit: self.limit,
            query: self.query.clone(),
            threshold: self.threshold,
        }
    }
}

/// Request body structure for REST API calls
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SearchRequestBody {
    pub limit: Option<i32>,
    pub query: Option<String>,
    pub threshold: Option<f64>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct SearchResponse {
    #[schemars(description = r#" - "#)]
    pub chunks: Option<Vec<serde_json::Value>>,
    #[schemars(description = r#" - "#)]
    pub query_time_ms: Option<i32>,
}

impl IntoContents for SearchResponse {
    fn into_contents(self) -> Vec<Content> {
        // Convert the response into a Vec<Content> as expected by MCP
        // Panics only if serialization fails, which should be impossible for valid structs
        vec![Content::json(self).expect("Failed to serialize SearchResponse to Content")]
    }
}

/// `/search` endpoint handler
/// Search code by meaning, not just text
/// Use this when you need to find code that implements a concept or pattern. This understands semantic meaning-searching for \"authentication\" will find login functions, JWT validation, password checking, etc. even if they don't contain the word \"authentication\". Perfect for exploring unfamiliar codebases or finding implementation patterns.
#[doc = r#"Verb: GET
Path: /search
Parameters: SearchParams
Responses:
    200: Successful Operation
    400: Bad input parameter
    500: Internal Server Error
    502: Bad Gateway
    503: Service Unavailable
    504: Gateway Timeout
Tag: untagged"#]
pub async fn search_handler(
    config: &Config,
    params: &SearchParams,
) -> Result<CallToolResult, agenterra_rmcp::Error> {
    // Log incoming request parameters and request details as structured JSON
    info!(
        target = "handler",
        event = "incoming_request",
        endpoint = "search",
        method = "POST",
        path = "/search",
        params = serde_json::to_string(params).unwrap_or_else(|e| {
            warn!("Failed to serialize request params: {e}");
            "{}".to_string()
        })
    );
    debug!(
        target = "handler",
        event = "before_api_call",
        endpoint = "search"
    );
    let request_body = match serde_json::to_value(params.to_request_body()) {
        Ok(val) => Some(val),
        Err(e) => {
            error!(
                target = "handler",
                event = "serialization_error",
                endpoint = "search",
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
        get_endpoint_response::<_, SearchResponse>(config, params, "POST", request_body).await;

    match &resp {
        Ok(r) => {
            info!(
                target = "handler",
                event = "api_response",
                endpoint = "search",
                response = ?r
            );
        }
        Err(e) => {
            error!(target = "handler", event = "api_error", endpoint = "search", error = ?e);
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
        let params = SearchParams {
            limit: None,
            query: None,
            threshold: None,
        };
        let _ = serde_json::to_string(&params).expect("Serializing test params should not fail");
    }
}
