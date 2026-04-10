// MCPlex — Router Module
// Intelligent tool routing to reduce context window bloat

pub mod keyword;
pub mod semantic;

use crate::config::{AppConfig, RouterStrategy};
use crate::protocol::RegisteredTool;

/// Trait for tool routing strategies
pub trait ToolRouter: Send + Sync {
    /// Route a query to the most relevant tools
    /// Returns a filtered, ranked subset of tools
    fn route(&self, query: &str, tools: &[RegisteredTool], top_k: usize) -> Vec<RegisteredTool>;

    /// Get the name of this router strategy
    fn name(&self) -> &str;
}

/// Passthrough router — returns all tools (no filtering)
pub struct PassthroughRouter;

impl ToolRouter for PassthroughRouter {
    fn route(&self, _query: &str, tools: &[RegisteredTool], _top_k: usize) -> Vec<RegisteredTool> {
        tools.to_vec()
    }

    fn name(&self) -> &str {
        "passthrough"
    }
}

/// Create a router based on configuration
pub fn create_router(config: &AppConfig) -> Box<dyn ToolRouter + Send + Sync> {
    match config.router.strategy {
        RouterStrategy::Semantic => Box::new(semantic::SemanticRouter::new(
            config.router.similarity_threshold,
            config.router.cache_embeddings,
        )),
        RouterStrategy::Keyword => Box::new(keyword::KeywordRouter::new(
            config.router.similarity_threshold,
        )),
        RouterStrategy::Passthrough => Box::new(PassthroughRouter),
    }
}
