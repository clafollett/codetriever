//! Do not edit by hand.
//! Auto-generated handler stubs for MCP endpoints.
// MCP auto-generated: Endpoint handler modules
pub mod clean;
pub mod compact;
pub mod get_context;
pub mod index;
pub mod search;
pub mod find_similar;
pub mod get_stats;
pub mod get_status;
pub mod find_usages;

// Internal dependencies
use crate::config::Config;

// External dependencies
use log::debug;
use agenterra_rmcp::{
    handler::server::tool::Parameters, model::*, service::*, tool, Error as McpError,
    ServerHandler,
};

#[derive(Clone)]
pub struct McpServer {
    tool_router: agenterra_rmcp::handler::server::router::tool::ToolRouter<McpServer>,
    config: Config,
}

impl McpServer {
    /// Create a new MCP server instance with default configuration
    pub fn new(config: Config) -> Self {
        Self {
            tool_router: Self::tool_router(),
            config,
        }
    }
}

#[agenterra_rmcp::tool_router]
impl McpServer {
    /// Returns MCP server status for Inspector/health validation
    #[tool(description = "Returns MCP server status for Inspector/health validation")]
    pub async fn ping(&self) -> Result<CallToolResult, McpError> {
        Ok(CallToolResult::success(vec![Content::text(
            "The MCP server is alive!",
        )]))
    }
    /// MCP API `/clean` endpoint handler
    #[tool(description = r#"Clean up outdated index entries - Maintenance operation to remove stale data. Use when: index size grows too large, after deleting many files, to remove entries older than X days, or when switching between branches frequently. Frees up disk space and improves search performance."#)]
    pub async fn clean(
        &self,
        Parameters(params): Parameters<clean::CleanParams>,
    ) -> Result<CallToolResult, McpError> {
        clean::clean_handler(&self.config, &params).await
    }
    /// MCP API `/compact` endpoint handler
    #[tool(description = r#"Optimize database for better performance - Runs database optimization to improve query speed and reduce file size. Similar to SQL Server index rebuild or PostgreSQL VACUUM. Use monthly or when /status shows degraded search performance. Safe to run anytime but may temporarily slow searches during compaction."#)]
    pub async fn compact(
        &self,
        Parameters(params): Parameters<compact::CompactParams>,
    ) -> Result<CallToolResult, McpError> {
        compact::compact_handler(&self.config, &params).await
    }
    /// MCP API `/get_context` endpoint handler
    #[tool(description = r#"Get surrounding code context for a specific location - Use this when you need to understand code in its full context. Given a file and line number, returns the surrounding code including function signatures, class definitions, imports, and nearby related code. Essential when you need to see the \"bigger picture\" around a specific piece of code."#)]
    pub async fn get_context(
        &self,
        Parameters(params): Parameters<get_context::GetContextParams>,
    ) -> Result<CallToolResult, McpError> {
        get_context::get_context_handler(&self.config, &params).await
    }
    /// MCP API `/index` endpoint handler
    #[tool(description = r#"Refresh the code index (usually automatic) - Triggers a reindex of the codebase. Usually runs automatically via file watcher, but use this when: switching branches and need immediate index update, after large refactoring, when status shows stale files, or to force a full rebuild. Returns immediately with job ID (async mode) or waits for completion (sync mode). Check progress via the /status endpoint."#)]
    pub async fn index(
        &self,
        Parameters(params): Parameters<index::IndexParams>,
    ) -> Result<CallToolResult, McpError> {
        index::index_handler(&self.config, &params).await
    }
    /// MCP API `/search` endpoint handler
    #[tool(description = r#"Search code by meaning, not just text - Use this when you need to find code that implements a concept or pattern. This understands semantic meaning-searching for \"authentication\" will find login functions, JWT validation, password checking, etc. even if they don't contain the word \"authentication\". Perfect for exploring unfamiliar codebases or finding implementation patterns."#)]
    pub async fn search(
        &self,
        Parameters(params): Parameters<search::SearchParams>,
    ) -> Result<CallToolResult, McpError> {
        search::search_handler(&self.config, &params).await
    }
    /// MCP API `/find_similar` endpoint handler
    #[tool(description = r#"Find code similar to a given snippet - Use this when you have an example of code and want to find similar implementations. Useful for: finding duplicated logic that could be refactored, locating all error handling patterns similar to one you're reviewing, or discovering variations of the same algorithm. Returns code chunks ranked by similarity score."#)]
    pub async fn find_similar(
        &self,
        Parameters(params): Parameters<find_similar::FindSimilarParams>,
    ) -> Result<CallToolResult, McpError> {
        find_similar::find_similar_handler(&self.config, &params).await
    }
    /// MCP API `/get_stats` endpoint handler
    #[tool(description = r#"Get quick index statistics - Lightweight endpoint for basic metrics. Use when you just need numbers: total files indexed, chunk count, database size, last update time. Faster than /status when you don't need detailed job information."#)]
    pub async fn get_stats(
        &self,
        Parameters(params): Parameters<get_stats::GetStatsParams>,
    ) -> Result<CallToolResult, McpError> {
        get_stats::get_stats_handler(&self.config, &params).await
    }
    /// MCP API `/get_status` endpoint handler
    #[tool(description = r#"Check health, index jobs, and performance metrics - Use this to understand the current state of the codetriever system. Shows: active indexing jobs and their progress, file watcher status, index freshness, performance metrics, and any errors. Check this when searches seem slow or outdated, before starting large operations, or to monitor background indexing."#)]
    pub async fn get_status(
        &self,
        Parameters(params): Parameters<get_status::GetStatusParams>,
    ) -> Result<CallToolResult, McpError> {
        get_status::get_status_handler(&self.config, &params).await
    }
    /// MCP API `/find_usages` endpoint handler
    #[tool(description = r#"Find all usages of a function, class, or variable - Use this to trace how a symbol is used throughout the codebase. Perfect for understanding impact of changes, finding all callers of a function, tracking down where a variable is modified, or analyzing dependencies. Distinguishes between definitions and references."#)]
    pub async fn find_usages(
        &self,
        Parameters(params): Parameters<find_usages::FindUsagesParams>,
    ) -> Result<CallToolResult, McpError> {
        find_usages::find_usages_handler(&self.config, &params).await
    }
}

#[agenterra_rmcp::tool_handler]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        debug!("[MCP] get_info() called - should show tools!");

        // Set up explicit capabilities for tools and resources
        let tools_capability = ToolsCapability {
            list_changed: Some(true),
        };

        let resources_capability = ResourcesCapability {
            list_changed: Some(true),
            ..ResourcesCapability::default()
        };

        let info = ServerInfo {
            protocol_version: ProtocolVersion::V_2024_11_05,
            capabilities: ServerCapabilities {
                experimental: None,
                logging: None,
                completions: None,
                prompts: None,
                resources: Some(resources_capability),
                tools: Some(tools_capability),
            },
            server_info: Implementation::from_build_env(),
            
            instructions: None,
            
        };

        debug!("[MCP] Returning ServerInfo with enabled tools and resources: {:?}", info);
        info
    }

    /// Implements MCP resource enumeration for all schema resources (one per endpoint)
    fn list_resources(
        &self, _request: Option<PaginatedRequestParam>, _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ListResourcesResult, McpError>> + Send + '_ {
        use agenterra_rmcp::model::{Annotated, RawResource};
        let resources = vec![
            Annotated {
                raw: RawResource {
                    uri: format!("/schema/{}", "clean"),
                    name: "clean".to_string(),
                    description: Some(
                        "JSON schema for the /clean endpoint (fields, types, docs, envelope)"
                            .to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                },
                annotations: Default::default(),
            },
            Annotated {
                raw: RawResource {
                    uri: format!("/schema/{}", "compact"),
                    name: "compact".to_string(),
                    description: Some(
                        "JSON schema for the /compact endpoint (fields, types, docs, envelope)"
                            .to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                },
                annotations: Default::default(),
            },
            Annotated {
                raw: RawResource {
                    uri: format!("/schema/{}", "get_context"),
                    name: "get_context".to_string(),
                    description: Some(
                        "JSON schema for the /get_context endpoint (fields, types, docs, envelope)"
                            .to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                },
                annotations: Default::default(),
            },
            Annotated {
                raw: RawResource {
                    uri: format!("/schema/{}", "index"),
                    name: "index".to_string(),
                    description: Some(
                        "JSON schema for the /index endpoint (fields, types, docs, envelope)"
                            .to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                },
                annotations: Default::default(),
            },
            Annotated {
                raw: RawResource {
                    uri: format!("/schema/{}", "search"),
                    name: "search".to_string(),
                    description: Some(
                        "JSON schema for the /search endpoint (fields, types, docs, envelope)"
                            .to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                },
                annotations: Default::default(),
            },
            Annotated {
                raw: RawResource {
                    uri: format!("/schema/{}", "find_similar"),
                    name: "find_similar".to_string(),
                    description: Some(
                        "JSON schema for the /find_similar endpoint (fields, types, docs, envelope)"
                            .to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                },
                annotations: Default::default(),
            },
            Annotated {
                raw: RawResource {
                    uri: format!("/schema/{}", "get_stats"),
                    name: "get_stats".to_string(),
                    description: Some(
                        "JSON schema for the /get_stats endpoint (fields, types, docs, envelope)"
                            .to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                },
                annotations: Default::default(),
            },
            Annotated {
                raw: RawResource {
                    uri: format!("/schema/{}", "get_status"),
                    name: "get_status".to_string(),
                    description: Some(
                        "JSON schema for the /get_status endpoint (fields, types, docs, envelope)"
                            .to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                },
                annotations: Default::default(),
            },
            Annotated {
                raw: RawResource {
                    uri: format!("/schema/{}", "find_usages"),
                    name: "find_usages".to_string(),
                    description: Some(
                        "JSON schema for the /find_usages endpoint (fields, types, docs, envelope)"
                            .to_string(),
                    ),
                    mime_type: Some("application/json".to_string()),
                    size: None,
                },
                annotations: Default::default(),
            },
        ];
        std::future::ready(Ok(ListResourcesResult { resources, next_cursor: None }))
    }

    /// Implements MCP resource fetching for schema resources by URI
    fn read_resource(
        &self,
        request: ReadResourceRequestParam,
        _context: RequestContext<RoleServer>,
    ) -> impl Future<Output = Result<ReadResourceResult, McpError>> + Send + '_ {
        use agenterra_rmcp::model::{ResourceContents, ReadResourceResult};
        let uri = request.uri;
        let prefix = "/schema/";
        let result = if let Some(endpoint) = uri.strip_prefix(prefix) {
            let ep_lower = endpoint.to_lowercase();
            let schema_json = match ep_lower.as_str() {
                "clean" => include_str!("../../schemas/clean.json"),
                "compact" => include_str!("../../schemas/compact.json"),
                "get_context" => include_str!("../../schemas/get_context.json"),
                "index" => include_str!("../../schemas/index.json"),
                "search" => include_str!("../../schemas/search.json"),
                "find_similar" => include_str!("../../schemas/find_similar.json"),
                "get_stats" => include_str!("../../schemas/get_stats.json"),
                "get_status" => include_str!("../../schemas/get_status.json"),
                "find_usages" => include_str!("../../schemas/find_usages.json"),
                _ => return std::future::ready(Err(McpError::resource_not_found(
                    format!("Schema not found for endpoint '{}': unknown endpoint", endpoint),
                    None,
                ))),
            };
            let resource =
                ResourceContents::text(schema_json, format!("/schema/{ep_lower}"));
            Ok(ReadResourceResult {
                contents: vec![resource],
            })
        } else {
            Err(McpError::resource_not_found(
                format!("Unknown resource URI: {uri}"),
                None,
            ))
        };
        std::future::ready(result)
    }
}
