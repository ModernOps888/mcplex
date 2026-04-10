// MCPlex — Semantic Router
// Embedding-based semantic tool routing for maximum context window savings
// Uses a lightweight local embedding approach (no external API required)

use std::collections::HashMap;
use std::sync::RwLock;
use tracing::debug;

use super::ToolRouter;
use crate::protocol::RegisteredTool;

/// Semantic router using character n-gram embeddings
/// This is a lightweight, zero-dependency approach that provides
/// better accuracy than keyword matching without requiring an ML model
pub struct SemanticRouter {
    threshold: f32,
    cache_enabled: bool,
    /// Cached embeddings: tool_fqn → embedding vector
    embedding_cache: RwLock<HashMap<String, Vec<f32>>>,
}

/// Embedding dimension for character n-grams
const EMBEDDING_DIM: usize = 256;
/// N-gram sizes to use
const NGRAM_SIZES: &[usize] = &[2, 3, 4];

impl SemanticRouter {
    pub fn new(threshold: f32, cache_enabled: bool) -> Self {
        Self {
            threshold,
            cache_enabled,
            embedding_cache: RwLock::new(HashMap::new()),
        }
    }

    /// Generate an embedding vector for text using character n-gram hashing
    /// This approach captures:
    /// - Partial word matches (subword information)
    /// - Typo resilience
    /// - Semantic proximity of related terms
    fn embed(&self, text: &str) -> Vec<f32> {
        let lower = text.to_lowercase();
        let mut vector = vec![0.0f32; EMBEDDING_DIM];

        // Word-level features
        let words: Vec<&str> = lower.split_whitespace().collect();
        for word in &words {
            // Character n-grams
            let chars: Vec<char> = word.chars().collect();
            for &n in NGRAM_SIZES {
                if chars.len() >= n {
                    for window in chars.windows(n) {
                        let ngram: String = window.iter().collect();
                        let hash = Self::hash_string(&ngram);
                        let idx = (hash % EMBEDDING_DIM as u64) as usize;
                        // Use hash sign to create both positive and negative values
                        let sign = if (hash >> 32).is_multiple_of(2) { 1.0 } else { -1.0 };
                        vector[idx] += sign;
                    }
                }
            }

            // Whole word hash (boosts exact matches)
            let hash = Self::hash_string(word);
            let idx = (hash % EMBEDDING_DIM as u64) as usize;
            vector[idx] += 2.0; // Stronger weight for whole words
        }

        // Word bigrams for phrase-level features
        for pair in words.windows(2) {
            let bigram = format!("{} {}", pair[0], pair[1]);
            let hash = Self::hash_string(&bigram);
            let idx = (hash % EMBEDDING_DIM as u64) as usize;
            vector[idx] += 1.5;
        }

        // L2 normalize
        let magnitude: f32 = vector.iter().map(|x| x * x).sum::<f32>().sqrt();
        if magnitude > 0.0 {
            for v in vector.iter_mut() {
                *v /= magnitude;
            }
        }

        vector
    }

    /// FNV-1a hash for consistent, fast hashing
    fn hash_string(s: &str) -> u64 {
        let mut hash: u64 = 0xcbf29ce484222325;
        for byte in s.bytes() {
            hash ^= byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        hash
    }

    /// Cosine similarity between two vectors
    fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
        let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

        if mag_a == 0.0 || mag_b == 0.0 {
            0.0
        } else {
            dot / (mag_a * mag_b)
        }
    }

    /// Get or compute embedding for a tool
    fn get_tool_embedding(&self, tool: &RegisteredTool) -> Vec<f32> {
        if self.cache_enabled {
            // Check cache
            if let Ok(cache) = self.embedding_cache.read() {
                if let Some(cached) = cache.get(&tool.fqn) {
                    return cached.clone();
                }
            }
        }

        // Build the tool text for embedding
        let text = build_tool_text(tool);
        let embedding = self.embed(&text);

        // Cache it
        if self.cache_enabled {
            if let Ok(mut cache) = self.embedding_cache.write() {
                cache.insert(tool.fqn.clone(), embedding.clone());
            }
        }

        embedding
    }
}

impl ToolRouter for SemanticRouter {
    fn route(&self, query: &str, tools: &[RegisteredTool], top_k: usize) -> Vec<RegisteredTool> {
        if tools.is_empty() || query.is_empty() {
            return tools.to_vec();
        }

        let query_embedding = self.embed(query);

        // Score all tools
        let mut scored: Vec<(usize, f32)> = tools
            .iter()
            .enumerate()
            .map(|(i, tool)| {
                let tool_embedding = self.get_tool_embedding(tool);
                let similarity = Self::cosine_similarity(&query_embedding, &tool_embedding);
                (i, similarity)
            })
            .filter(|(_, score)| *score >= self.threshold)
            .collect();

        // Sort by similarity descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top_k
        let selected: Vec<RegisteredTool> = scored
            .iter()
            .take(top_k)
            .map(|(i, score)| {
                debug!("  🧠 {} (similarity: {:.3})", tools[*i].fqn, score);
                tools[*i].clone()
            })
            .collect();

        selected
    }

    fn name(&self) -> &str {
        "semantic"
    }
}

/// Build searchable text from a tool definition
fn build_tool_text(tool: &RegisteredTool) -> String {
    let mut text = format!("{} {}", tool.definition.name, tool.server_name);

    // Expand camelCase and snake_case names
    let expanded = expand_name(&tool.definition.name);
    text.push(' ');
    text.push_str(&expanded);

    if let Some(ref desc) = tool.definition.description {
        text.push(' ');
        text.push_str(desc);
    }

    // Include parameter names and descriptions from the schema
    if let Some(ref schema) = tool.definition.input_schema {
        if let Some(props) = schema.get("properties") {
            if let Some(obj) = props.as_object() {
                for (key, value) in obj {
                    text.push(' ');
                    text.push_str(key);
                    text.push(' ');
                    text.push_str(&expand_name(key));

                    if let Some(desc) = value.get("description") {
                        if let Some(s) = desc.as_str() {
                            text.push(' ');
                            text.push_str(s);
                        }
                    }
                }
            }
        }
    }

    text
}

/// Expand camelCase and snake_case names into separate words
fn expand_name(name: &str) -> String {
    let mut words = Vec::new();
    let mut current_word = String::new();

    for ch in name.chars() {
        if ch == '_' || ch == '-' {
            if !current_word.is_empty() {
                words.push(current_word.clone());
                current_word.clear();
            }
        } else if ch.is_uppercase() && !current_word.is_empty() {
            words.push(current_word.clone());
            current_word.clear();
            current_word.push(ch.to_lowercase().next().unwrap_or(ch));
        } else {
            current_word.push(ch.to_lowercase().next().unwrap_or(ch));
        }
    }
    if !current_word.is_empty() {
        words.push(current_word);
    }

    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ToolDefinition;

    fn make_tool(name: &str, desc: &str, server: &str) -> RegisteredTool {
        RegisteredTool::new(
            ToolDefinition {
                name: name.to_string(),
                description: Some(desc.to_string()),
                input_schema: None,
            },
            server,
        )
    }

    #[test]
    fn test_semantic_router_relevance() {
        let router = SemanticRouter::new(0.0, true);
        let tools = vec![
            make_tool(
                "create_issue",
                "Create a new issue in the bug tracker",
                "github",
            ),
            make_tool("send_message", "Send a message to a Slack channel", "slack"),
            make_tool(
                "query_database",
                "Run SQL queries against the production database",
                "database",
            ),
            make_tool(
                "list_pull_requests",
                "List pull requests in a repository",
                "github",
            ),
            make_tool(
                "search_code",
                "Search for code patterns in repositories",
                "github",
            ),
        ];

        let results = router.route("I want to create a bug report", &tools, 2);
        assert!(!results.is_empty());
        // create_issue should rank highest for "bug report"
        assert_eq!(results[0].definition.name, "create_issue");
    }

    #[test]
    fn test_embedding_similarity() {
        let router = SemanticRouter::new(0.0, false);

        let embed_a = router.embed("create github issue");
        let embed_b = router.embed("create issue bug tracker");
        let embed_c = router.embed("send slack message");

        let sim_ab = SemanticRouter::cosine_similarity(&embed_a, &embed_b);
        let sim_ac = SemanticRouter::cosine_similarity(&embed_a, &embed_c);

        // "create issue" should be more similar to "create issue bug tracker"
        // than to "send slack message"
        assert!(sim_ab > sim_ac, "Expected {} > {}", sim_ab, sim_ac);
    }

    #[test]
    fn test_expand_name() {
        assert_eq!(expand_name("createIssue"), "create issue");
        assert_eq!(expand_name("create_issue"), "create issue");
        assert_eq!(expand_name("listPullRequests"), "list pull requests");
    }

    #[test]
    fn test_caching() {
        let router = SemanticRouter::new(0.0, true);
        let tool = make_tool("test_tool", "A test tool", "test");

        // First call computes
        let e1 = router.get_tool_embedding(&tool);
        // Second call should hit cache
        let e2 = router.get_tool_embedding(&tool);

        assert_eq!(e1, e2);
    }
}
