// MCPlex — Semantic Router
// Embedding-based semantic tool routing for maximum context window savings
// Uses a lightweight local embedding approach (no external API required)
//
// v0.2.1: IDF-weighted n-grams + server-name boost for improved ranking quality

use std::collections::HashMap;
use std::sync::RwLock;
use tracing::debug;

use super::ToolRouter;
use crate::protocol::RegisteredTool;

/// Semantic router using character n-gram embeddings with IDF weighting
///
/// Improvements over raw n-gram hashing:
/// - **IDF weighting**: Tokens appearing in many tool descriptions (e.g., "search")
///   contribute less; tokens unique to fewer tools (e.g., "memory") contribute more.
/// - **Server-name boost**: When the query contains a token matching a server name,
///   tools from that server receive a ranking multiplier (default 1.3x).
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
/// Multiplier applied to tools whose server name matches a token in the query
const SERVER_NAME_BOOST: f32 = 1.3;

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
        self.embed_weighted(text, None)
    }

    /// Generate an IDF-weighted embedding vector for text.
    ///
    /// When `idf_weights` is provided, each token's contribution to the vector
    /// is scaled by its inverse document frequency. Tokens that appear across
    /// many tool descriptions contribute almost nothing; rare tokens dominate.
    fn embed_weighted(&self, text: &str, idf_weights: Option<&HashMap<String, f32>>) -> Vec<f32> {
        let lower = text.to_lowercase();
        let mut vector = vec![0.0f32; EMBEDDING_DIM];

        // Word-level features
        let words: Vec<&str> = lower.split_whitespace().collect();
        for word in &words {
            // Look up IDF weight for this word (default 1.0 if no IDF map provided)
            let word_idf = idf_weights
                .and_then(|w| w.get(*word))
                .copied()
                .unwrap_or(1.0);

            // Character n-grams
            let chars: Vec<char> = word.chars().collect();
            for &n in NGRAM_SIZES {
                if chars.len() >= n {
                    for window in chars.windows(n) {
                        let ngram: String = window.iter().collect();
                        let hash = Self::hash_string(&ngram);
                        let idx = (hash % EMBEDDING_DIM as u64) as usize;
                        // Use hash sign to create both positive and negative values
                        let sign = if (hash >> 32).is_multiple_of(2) {
                            1.0
                        } else {
                            -1.0
                        };
                        vector[idx] += sign * word_idf;
                    }
                }
            }

            // Whole word hash (boosts exact matches)
            let hash = Self::hash_string(word);
            let idx = (hash % EMBEDDING_DIM as u64) as usize;
            vector[idx] += 2.0 * word_idf; // Stronger weight for whole words, scaled by IDF
        }

        // Word bigrams for phrase-level features
        for pair in words.windows(2) {
            let bigram = format!("{} {}", pair[0], pair[1]);

            // Bigram IDF: use the average of both words' IDF values
            let bigram_idf = idf_weights
                .map(|w| {
                    let idf_a = w.get(pair[0]).copied().unwrap_or(1.0);
                    let idf_b = w.get(pair[1]).copied().unwrap_or(1.0);
                    (idf_a + idf_b) / 2.0
                })
                .unwrap_or(1.0);

            let hash = Self::hash_string(&bigram);
            let idx = (hash % EMBEDDING_DIM as u64) as usize;
            vector[idx] += 1.5 * bigram_idf;
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

    /// Get or compute embedding for a tool (used for non-IDF path / caching)
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

    /// Compute IDF weights across all tool descriptions.
    ///
    /// Returns a map of token → IDF score where:
    /// - Tokens appearing in all documents have low IDF (~1.0)
    /// - Tokens appearing in few documents have high IDF
    ///
    /// Formula: IDF(t) = ln(N / (df(t) + 1)) + 1.0
    fn compute_idf(tools: &[RegisteredTool]) -> HashMap<String, f32> {
        let total_docs = tools.len() as f32;
        let mut doc_freq: HashMap<String, usize> = HashMap::new();

        // Count how many documents each token appears in
        for tool in tools {
            let text = build_tool_text(tool);
            let lower = text.to_lowercase();
            let words: std::collections::HashSet<String> =
                lower.split_whitespace().map(|s| s.to_string()).collect();

            for word in words {
                *doc_freq.entry(word).or_insert(0) += 1;
            }
        }

        // Convert document frequency to IDF
        doc_freq
            .iter()
            .map(|(token, df)| {
                let idf_val = (total_docs / (*df as f32 + 1.0)).ln() + 1.0;
                (token.clone(), idf_val)
            })
            .collect()
    }

    /// Extract unique server names from the tool set (lowercased)
    fn extract_server_names(tools: &[RegisteredTool]) -> std::collections::HashSet<String> {
        tools.iter().map(|t| t.server_name.to_lowercase()).collect()
    }
}

impl ToolRouter for SemanticRouter {
    fn route(&self, query: &str, tools: &[RegisteredTool], top_k: usize) -> Vec<RegisteredTool> {
        if tools.is_empty() || query.is_empty() {
            return tools.to_vec();
        }

        // Compute IDF weights across all tool descriptions
        let idf_weights = Self::compute_idf(tools);

        // Embed the query with IDF weighting
        let query_embedding = self.embed_weighted(query, Some(&idf_weights));

        // Determine which server names appear in the query (for server-name boost)
        let server_names = Self::extract_server_names(tools);
        let query_lower = query.to_lowercase();
        let query_tokens: std::collections::HashSet<&str> =
            query_lower.split_whitespace().collect();

        let matched_servers: std::collections::HashSet<&str> = server_names
            .iter()
            .filter(|name| query_tokens.contains(name.as_str()))
            .map(|s| s.as_str())
            .collect();

        // Compute average document length for BM25-style length normalization.
        // This compensates for the cosine-similarity dilution effect where long
        // descriptions spread their signal across more n-gram dimensions, giving
        // short-description tools an unfair similarity advantage.
        let tool_texts: Vec<String> = tools.iter().map(build_tool_text).collect();
        let avg_doc_len: f32 = if tools.is_empty() {
            1.0
        } else {
            tool_texts
                .iter()
                .map(|t| t.split_whitespace().count() as f32)
                .sum::<f32>()
                / tools.len() as f32
        };

        /// BM25 length normalization parameter (0.0 = no normalization, 1.0 = full).
        /// 0.3 is a mild correction that fixes ranking inversions without
        /// over-boosting verbose tool descriptions.
        const BM25_B: f32 = 0.3;

        // Score all tools
        let mut scored: Vec<(usize, f32)> = tools
            .iter()
            .enumerate()
            .map(|(i, tool)| {
                // Embed the tool text with IDF weighting
                let tool_embedding = self.embed_weighted(&tool_texts[i], Some(&idf_weights));
                let mut similarity = Self::cosine_similarity(&query_embedding, &tool_embedding);

                // BM25-style length normalization: compensate for cosine dilution
                // in long documents. A factor > 1.0 for long docs (boost),
                // < 1.0 for short docs (reduce).
                let doc_len = tool_texts[i].split_whitespace().count() as f32;
                let length_norm = 1.0 + BM25_B * ((doc_len / avg_doc_len) - 1.0);
                similarity *= length_norm.max(0.5); // clamp to prevent extreme values

                // Apply server-name boost: if the query mentions a server name and
                // this tool belongs to that server, amplify its score
                if !matched_servers.is_empty() {
                    let tool_server = tool.server_name.to_lowercase();
                    if matched_servers.contains(tool_server.as_str()) {
                        similarity *= SERVER_NAME_BOOST;
                        debug!(
                            "  🎯 Server-name boost applied to {} (server: {})",
                            tool.fqn, tool.server_name
                        );
                    }
                }

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

    /// Regression test for issue #3: memory_search should rank above search_code
    /// when the query is clearly about searching a memory database.
    ///
    /// Root cause was surface-token collision on "search" — both tools match
    /// on that common word. With IDF weighting, "search" (high doc frequency)
    /// contributes almost nothing, while "memory" (low doc frequency) dominates.
    /// The server-name boost further amplifies tools from the "memory" server
    /// when "memory" appears in the query.
    #[test]
    fn test_memory_search_ranks_above_search_code() {
        let router = SemanticRouter::new(0.0, false);
        let tools = vec![
            make_tool(
                "search_code",
                "Search for code patterns in repositories and codebases",
                "code-context",
            ),
            make_tool(
                "memory_health",
                "Check the health status of the memory service backend",
                "memory",
            ),
            make_tool(
                "memory_search",
                "Search the memory database for stored entries matching a query",
                "memory",
            ),
            make_tool(
                "memory_cleanup",
                "Remove old or duplicate entries from the memory store",
                "memory",
            ),
            make_tool(
                "index_codebase",
                "Index a codebase for semantic code search",
                "code-context",
            ),
        ];

        let results = router.route(
            "search my memory database for entries about mcplex deployment",
            &tools,
            5,
        );

        assert!(!results.is_empty());
        // memory_search should be the top result
        assert_eq!(
            results[0].definition.name,
            "memory_search",
            "Expected memory_search to rank #1, got: {:?}",
            results
                .iter()
                .map(|t| &t.definition.name)
                .collect::<Vec<_>>()
        );
    }

    /// Verify IDF computation produces sensible weights
    #[test]
    fn test_idf_computation() {
        let tools = vec![
            make_tool("search_code", "Search code in repositories", "github"),
            make_tool("search_memory", "Search the memory database", "memory"),
            make_tool("create_issue", "Create a new GitHub issue", "github"),
        ];

        let idf = SemanticRouter::compute_idf(&tools);

        // "search" appears in 2/3 tools → lower IDF
        // "memory" appears in 1/3 tools → higher IDF
        let search_idf = idf.get("search").unwrap_or(&1.0);
        let memory_idf = idf.get("memory").unwrap_or(&1.0);

        assert!(
            memory_idf > search_idf,
            "Expected 'memory' IDF ({}) > 'search' IDF ({})",
            memory_idf,
            search_idf
        );
    }
}
