// MCPlex — Keyword Router
// TF-IDF-based keyword matching for zero-dependency tool routing

use std::collections::HashMap;
use tracing::debug;

use super::ToolRouter;
use crate::protocol::RegisteredTool;

/// Keyword-based router using TF-IDF scoring
/// Zero external dependencies — works without any ML model
pub struct KeywordRouter {
    threshold: f32,
}

impl KeywordRouter {
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }
}

impl ToolRouter for KeywordRouter {
    fn route(&self, query: &str, tools: &[RegisteredTool], top_k: usize) -> Vec<RegisteredTool> {
        if tools.is_empty() || query.is_empty() {
            return tools.to_vec();
        }

        let query_tokens = tokenize(query);
        if query_tokens.is_empty() {
            return tools.to_vec();
        }

        // Build per-document term frequencies
        let mut doc_tokens: Vec<Vec<String>> = Vec::new();
        for tool in tools {
            let text = build_tool_text(tool);
            doc_tokens.push(tokenize(&text));
        }

        // Build IDF (inverse document frequency)
        let total_docs = doc_tokens.len() as f32;
        let mut doc_freq: HashMap<String, usize> = HashMap::new();
        for tokens in &doc_tokens {
            let unique: std::collections::HashSet<&String> = tokens.iter().collect();
            for token in unique {
                *doc_freq.entry(token.clone()).or_insert(0) += 1;
            }
        }

        let idf: HashMap<String, f32> = doc_freq
            .iter()
            .map(|(token, df)| {
                let idf_val = (total_docs / (*df as f32 + 1.0)).ln() + 1.0;
                (token.clone(), idf_val)
            })
            .collect();

        // Score each tool against the query
        let mut scored: Vec<(usize, f32)> = doc_tokens
            .iter()
            .enumerate()
            .map(|(i, tokens)| {
                let score = tfidf_cosine_similarity(&query_tokens, tokens, &idf);
                (i, score)
            })
            .filter(|(_, score)| *score >= self.threshold)
            .collect();

        // Sort by score descending
        scored.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        // Take top_k
        let selected: Vec<RegisteredTool> = scored
            .iter()
            .take(top_k)
            .map(|(i, score)| {
                debug!("  📌 {} (score: {:.3})", tools[*i].fqn, score);
                tools[*i].clone()
            })
            .collect();

        selected
    }

    fn name(&self) -> &str {
        "keyword"
    }
}

/// Build searchable text from a tool definition
fn build_tool_text(tool: &RegisteredTool) -> String {
    let mut text = format!("{} {}", tool.definition.name, tool.server_name);
    if let Some(ref desc) = tool.definition.description {
        text.push(' ');
        text.push_str(desc);
    }
    // Include parameter names from the schema
    if let Some(ref schema) = tool.definition.input_schema {
        if let Some(props) = schema.get("properties") {
            if let Some(obj) = props.as_object() {
                for key in obj.keys() {
                    text.push(' ');
                    text.push_str(key);
                }
                // Also include parameter descriptions
                for value in obj.values() {
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

/// Tokenize text into lowercase words, filtering stopwords
fn tokenize(text: &str) -> Vec<String> {
    let stopwords: std::collections::HashSet<&str> = [
        "a", "an", "the", "is", "are", "was", "were", "be", "been", "being", "have", "has", "had",
        "do", "does", "did", "will", "would", "could", "should", "may", "might", "can", "shall",
        "to", "of", "in", "for", "on", "with", "at", "by", "from", "as", "into", "through",
        "during", "before", "after", "above", "below", "between", "out", "off", "over", "under",
        "again", "further", "then", "once", "here", "there", "when", "where", "why", "how", "all",
        "both", "each", "few", "more", "most", "other", "some", "such", "no", "not", "only", "own",
        "same", "so", "than", "too", "very", "just", "or", "and", "but", "if", "this", "that",
        "these", "those", "it", "its",
    ]
    .into_iter()
    .collect();

    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric() && c != '_')
        .filter(|s| s.len() > 1 && !stopwords.contains(s))
        .map(|s| s.to_string())
        .collect()
}

/// Compute TF-IDF cosine similarity between query and document tokens
fn tfidf_cosine_similarity(
    query_tokens: &[String],
    doc_tokens: &[String],
    idf: &HashMap<String, f32>,
) -> f32 {
    // Build TF-IDF vectors
    let query_tf = term_freq(query_tokens);
    let doc_tf = term_freq(doc_tokens);

    // Collect all terms
    let mut all_terms: std::collections::HashSet<&String> = std::collections::HashSet::new();
    all_terms.extend(query_tf.keys());
    all_terms.extend(doc_tf.keys());

    let mut dot_product = 0.0f32;
    let mut query_magnitude = 0.0f32;
    let mut doc_magnitude = 0.0f32;

    for term in all_terms {
        let q_tfidf = query_tf.get(term).unwrap_or(&0.0) * idf.get(term).unwrap_or(&1.0);
        let d_tfidf = doc_tf.get(term).unwrap_or(&0.0) * idf.get(term).unwrap_or(&1.0);

        dot_product += q_tfidf * d_tfidf;
        query_magnitude += q_tfidf * q_tfidf;
        doc_magnitude += d_tfidf * d_tfidf;
    }

    let magnitude = query_magnitude.sqrt() * doc_magnitude.sqrt();
    if magnitude == 0.0 {
        0.0
    } else {
        dot_product / magnitude
    }
}

/// Compute term frequency (normalized)
fn term_freq(tokens: &[String]) -> HashMap<String, f32> {
    let mut freq: HashMap<String, f32> = HashMap::new();
    for token in tokens {
        *freq.entry(token.clone()).or_insert(0.0) += 1.0;
    }
    let max_freq = freq.values().cloned().fold(0.0f32, f32::max);
    if max_freq > 0.0 {
        for v in freq.values_mut() {
            *v /= max_freq;
        }
    }
    freq
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
    fn test_keyword_router_basic() {
        let router = KeywordRouter::new(0.0);
        let tools = vec![
            make_tool("create_issue", "Create a new GitHub issue", "github"),
            make_tool("send_message", "Send a Slack message to a channel", "slack"),
            make_tool(
                "query_database",
                "Execute a SQL query on the database",
                "database",
            ),
            make_tool("list_repos", "List GitHub repositories", "github"),
        ];

        let results = router.route("create a github issue", &tools, 2);
        assert!(!results.is_empty());
        assert_eq!(results[0].definition.name, "create_issue");
    }

    #[test]
    fn test_keyword_router_empty_query() {
        let router = KeywordRouter::new(0.0);
        let tools = vec![make_tool("test", "A test tool", "test")];
        let results = router.route("", &tools, 5);
        assert_eq!(results.len(), 1); // Returns all tools for empty query
    }

    #[test]
    fn test_tokenizer() {
        let tokens = tokenize("Create a new GitHub issue tracker");
        assert!(tokens.contains(&"create".to_string()));
        assert!(tokens.contains(&"github".to_string()));
        assert!(!tokens.contains(&"a".to_string())); // Stopword
    }
}
