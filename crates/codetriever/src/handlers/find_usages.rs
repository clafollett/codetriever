//! Auto-generated handler for `/find_usages` endpoint.

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

/// Auto-generated unified parameters struct for `/find_usages` endpoint.
/// Combines query parameters and request body properties into a single MCP interface.
/// Spec:
#[derive(Clone, Debug, Default, Deserialize, Serialize, JsonSchema, ToSchema)]
pub struct FindUsagesParams {
    #[schemars(description = r#"Request body property"#)]
    pub symbol: Option<String>,
    #[schemars(description = r#"Request body property"#)]
    pub usage_type: Option<String>,
}

// Implement Endpoint for generic handler
impl Endpoint for FindUsagesParams {
    fn path() -> &'static str {
        "/usages"
    }

    fn get_params(&self) -> HashMap<String, String> {
        HashMap::new()
    }
}

impl FindUsagesParams {
    /// Extract request body properties for REST API calls
    pub fn to_request_body(&self) -> FindUsagesRequestBody {
        FindUsagesRequestBody {
            symbol: self.symbol.clone(),
            usage_type: self.usage_type.clone(),
        }
    }
}

/// Request body structure for REST API calls
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FindUsagesRequestBody {
    pub symbol: Option<String>,
    pub usage_type: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, JsonSchema, ToSchema)]
pub struct FindUsagesResponse {
    #[schemars(description = r#" - "#)]
    pub usages: Option<Vec<serde_json::Value>>,
}

impl IntoContents for FindUsagesResponse {
    fn into_contents(self) -> Vec<Content> {
        // Convert the response into a Vec<Content> as expected by MCP
        // Panics only if serialization fails, which should be impossible for valid structs
        vec![Content::json(self).expect("Failed to serialize FindUsagesResponse to Content")]
    }
}

/// `/usages` endpoint handler
/// Find all usages of a function, class, or variable
/// Use this to trace how a symbol is used throughout the codebase. Perfect for understanding impact of changes, finding all callers of a function, tracking down where a variable is modified, or analyzing dependencies. Distinguishes between definitions and references.
#[doc = r#"Verb: GET
Path: /usages
Parameters: FindUsagesParams
Responses:
    200: Successful Operation
    400: Bad input parameter
    500: Internal Server Error
    502: Bad Gateway
    503: Service Unavailable
    504: Gateway Timeout
Tag: untagged"#]
pub async fn find_usages_handler(
    config: &Config,
    params: &FindUsagesParams,
) -> Result<CallToolResult, agenterra_rmcp::Error> {
    // Log incoming request parameters and request details as structured JSON
    info!(
        target = "handler",
        event = "incoming_request",
        endpoint = "find_usages",
        method = "POST",
        path = "/usages",
        params = serde_json::to_string(params).unwrap_or_else(|e| {
            warn!("Failed to serialize request params: {e}");
            "{}".to_string()
        })
    );
    debug!(
        target = "handler",
        event = "before_api_call",
        endpoint = "find_usages"
    );
    let request_body = match serde_json::to_value(params.to_request_body()) {
        Ok(val) => Some(val),
        Err(e) => {
            error!(
                target = "handler",
                event = "serialization_error",
                endpoint = "find_usages",
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
        get_endpoint_response::<_, FindUsagesResponse>(config, params, "POST", request_body).await;

    match &resp {
        Ok(r) => {
            info!(
                target = "handler",
                event = "api_response",
                endpoint = "find_usages",
                response = ?r
            );
        }
        Err(e) => {
            error!(target = "handler", event = "api_error", endpoint = "find_usages", error = ?e);
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
        let params = FindUsagesParams {
            symbol: None,
            usage_type: None,
        };
        let _ = serde_json::to_string(&params).expect("Serializing test params should not fail");
    }
}
