// Internal imports (std, crate)
use crate::config::Config;
use std::collections::HashMap;

// Public/external imports (alphabetized)
use agenterra_rmcp::model::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::json;

/// Trait to associate a parameter type with its endpoint path.
pub trait Endpoint {
    fn path() -> &'static str;
    fn get_params(&self) -> HashMap<String, String>;
}

/// Proxies query parameters and endpoint-specific parameters to the API, executes the proxied HTTP request.
/// Returns the result or our local ProxyError.
pub async fn get_endpoint_response<E, R>(
    config: &Config,
    endpoint: &E,
) -> Result<R, agenterra_rmcp::Error>
where
    E: Endpoint + Clone + Send + Sync,
    R: Serialize + DeserializeOwned,
{
    // Clone params to allow modification without affecting caller's original
    let mut params = endpoint.get_params();
    let client = reqwest::Client::new();

    // Build URL with path parameter substitution
    let mut path = <E as Endpoint>::path().to_string();
    let mut path_params_used = Vec::new();

    // Replace {paramName} placeholders in path with actual values
    for (key, value) in &params {
        let placeholder = format!("{{{}}}", key);
        if path.contains(&placeholder) {
            path = path.replace(&placeholder, value);
            path_params_used.push(key.clone());
        }
    }

    // Remove path parameters from query params since they're now in the URL
    for key in &path_params_used {
        params.remove(key);
    }

    let url = format!(
        "{}/{}",
        config.api_url.trim_end_matches('/'),
        path.trim_start_matches('/')
    );

    log::debug!("Sending request: URL={}, Query={:?}", url, params);

    // --- Execute Request ---
    let res = client
        .get(&url)
        .query(&params)
        .send()
        .await
        .map_err(reqwest_to_rmcp_error)?;

    let status = res.status();
    log::debug!("Received response status: {}", status);

    // Get response body
    let bytes = res.bytes().await.map_err(reqwest_to_rmcp_error)?;

    // --- Parse Response ---
    match serde_json::from_slice::<serde_json::Value>(&bytes) {
        Ok(val) => {
            log::debug!("Successfully parsed JSON response");
            if status.is_client_error() || status.is_server_error() {
                // Try to extract the most informative error message from error response
                let title = val.get("title").and_then(|v| v.as_str());
                let detail = val.get("detail").and_then(|v| v.as_str());
                let message = match (title, detail) {
                    (Some(t), Some(d)) => format!("{t}: {d}"),
                    (Some(t), None) => t.to_string(),
                    (None, Some(d)) => d.to_string(),
                    _ => val
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown API error")
                        .to_string(),
                };
                log::warn!("API returned error status {status}: {message}");
                let custom_code = format!("API_ERROR_{}", status.as_u16());
                let error_data = ErrorData::new(
                    ErrorCode::INTERNAL_ERROR,
                    message,
                    Some(json!({
                        "source": "api",
                        "original_code": custom_code,
                        "status": status.as_u16(),
                        "raw": val
                    })),
                );
                return Err(agenterra_rmcp::Error::from(error_data));
            }

            let parsed: R = serde_json::from_value(val).map_err(|e| {
                agenterra_rmcp::model::ErrorData::new(
                    agenterra_rmcp::model::ErrorCode::INTERNAL_ERROR,
                    format!("Failed to deserialize API response: {e}"),
                    None,
                )
            })?;

            Ok(parsed)
        }
        Err(e) => {
            log::error!(
                "Failed to parse response as JSON: {}. Status: {}",
                e,
                status
            );
            Err(serde_json_to_rmcp_error(e))
        }
    }
}

// Map reqwest errors to agenterra_rmcp::Error
fn reqwest_to_rmcp_error(e: reqwest::Error) -> agenterra_rmcp::Error {
    let message = e.to_string();
    let status = e.status().map(|s| s.as_u16());
    let custom_code_str = match e {
        _ if e.is_connect() => "NETWORK_CONNECTION_ERROR",
        _ if e.is_timeout() => "NETWORK_TIMEOUT_ERROR",
        _ if e.is_request() => "HTTP_REQUEST_ERROR",
        _ if e.is_status() => "HTTP_STATUS_ERROR",
        _ if e.is_body() | e.is_decode() => "HTTP_RESPONSE_BODY_ERROR",
        _ => "API_PROXY_ERROR",
    };

    let error_data = ErrorData::new(
        ErrorCode::INTERNAL_ERROR,
        message,
        Some(json!({
            "source": "reqwest",
            "original_code": custom_code_str,
            "status": status,
        })),
    );

    agenterra_rmcp::Error::from(error_data)
}

// Map serde_json errors to agenterra_rmcp::Error
fn serde_json_to_rmcp_error(e: serde_json::Error) -> agenterra_rmcp::Error {
    let error_data = ErrorData::new(
        ErrorCode::INVALID_PARAMS,
        e.to_string(),
        Some(json!({
            "source": "serde_json",
            "original_code": "JSON_PARSING_ERROR",
            "line": e.line(),
            "column": e.column(),
        })),
    );
    agenterra_rmcp::Error::from(error_data)
}
