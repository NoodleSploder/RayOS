//! Semantic Query Interface
//!
//! Natural language file retrieval with query parsing, expansion, and multi-factor ranking.
//!
//! ## Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                        Natural Language Query                           │
//! │        "find the python scripts about machine learning from last week"  │
//! └────────────────────────────────┬────────────────────────────────────────┘
//!                                  │
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         Query Parser                                     │
//! │  Intent: find | Entities: python, ML | Modifiers: recent (last week)    │
//! └────────────────────────────────┬────────────────────────────────────────┘
//!                                  │
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                        Query Expander                                    │
//! │  "machine learning" → [ML, deep learning, neural network, AI, ...]      │
//! └────────────────────────────────┬────────────────────────────────────────┘
//!                                  │
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                      Vector Store Search                                 │
//! │  HNSW similarity search with expanded query embeddings                  │
//! └────────────────────────────────┬────────────────────────────────────────┘
//!                                  │
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                       Multi-Factor Ranker                                │
//! │  Combine: similarity + recency + access_freq + context_relevance        │
//! └────────────────────────────────┬────────────────────────────────────────┘
//!                                  │
//!                                  ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                      Result Presenter                                    │
//! │  Format with explanations, snippets, and confidence scores              │
//! └─────────────────────────────────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```no_run
//! use rayos_volume::query_interface::{SemanticQueryEngine, QueryContext};
//! use rayos_volume::SemanticFS;
//!
//! #[tokio::main]
//! async fn main() -> anyhow::Result<()> {
//!     let fs = SemanticFS::new(Default::default()).await?;
//!     let engine = SemanticQueryEngine::new(fs);
//!
//!     // Simple query
//!     let results = engine.query("machine learning models").await?;
//!
//!     // Query with context
//!     let ctx = QueryContext::new()
//!         .with_current_project("ml-research")
//!         .with_recency_boost(0.3);
//!     let results = engine.query_with_context("training scripts", ctx).await?;
//!
//!     for result in results.items {
//!         println!("{}: {:.0}% - {}",
//!             result.path.display(),
//!             result.confidence * 100.0,
//!             result.explanation);
//!     }
//!
//!     Ok(())
//! }
//! ```

use crate::embedder::Embedder;
use crate::fs::SemanticFS;
use crate::types::{FileType, Query, SearchQuery, SearchResult};
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

// =============================================================================
// Query Types
// =============================================================================

/// The intent behind a query
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryIntent {
    /// Find files matching a description
    Find,
    /// List files in a category or location
    List,
    /// Compare two or more files
    Compare,
    /// Show files similar to another
    Similar,
    /// Show recently accessed/modified files
    Recent,
    /// Show files by a specific author/creator
    ByAuthor,
    /// Generic search (fallback)
    Search,
}

/// Time range for temporal queries
#[derive(Debug, Clone, Copy)]
pub struct TimeRange {
    /// Start of range (Unix timestamp)
    pub start: u64,
    /// End of range (Unix timestamp)
    pub end: u64,
}

impl TimeRange {
    /// Last N days
    pub fn last_days(days: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            start: now - (days * 24 * 60 * 60),
            end: now,
        }
    }

    /// Last N hours
    pub fn last_hours(hours: u64) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        Self {
            start: now - (hours * 60 * 60),
            end: now,
        }
    }

    /// This week
    pub fn this_week() -> Self {
        Self::last_days(7)
    }

    /// This month
    pub fn this_month() -> Self {
        Self::last_days(30)
    }
}

/// Parsed semantic query
#[derive(Debug, Clone)]
pub struct ParsedQuery {
    /// Original query text
    pub original: String,
    /// Detected intent
    pub intent: QueryIntent,
    /// Core search terms (after removing modifiers)
    pub core_terms: Vec<String>,
    /// Expanded terms (synonyms, related)
    pub expanded_terms: Vec<String>,
    /// File type filter
    pub file_type: Option<FileType>,
    /// Time range filter
    pub time_range: Option<TimeRange>,
    /// Specific path/directory filter
    pub path_filter: Option<String>,
    /// Tags to filter by
    pub tags: Vec<String>,
    /// Whether to boost recent files
    pub recency_boost: bool,
    /// Reference file (for "similar to" queries)
    pub reference_file: Option<PathBuf>,
}

impl ParsedQuery {
    /// Create a simple parsed query
    pub fn simple(query: &str) -> Self {
        Self {
            original: query.to_string(),
            intent: QueryIntent::Search,
            core_terms: query.split_whitespace().map(String::from).collect(),
            expanded_terms: Vec::new(),
            file_type: None,
            time_range: None,
            path_filter: None,
            tags: Vec::new(),
            recency_boost: false,
            reference_file: None,
        }
    }
}

/// Context for the query (current state, preferences)
#[derive(Debug, Clone, Default)]
pub struct QueryContext {
    /// Current working project/directory
    pub current_project: Option<String>,
    /// Recently accessed files (for context boosting)
    pub recent_files: Vec<PathBuf>,
    /// User's role/persona
    pub user_role: Option<String>,
    /// Weight for recency in ranking (0-1)
    pub recency_weight: f32,
    /// Weight for access frequency in ranking (0-1)
    pub frequency_weight: f32,
    /// Weight for context relevance in ranking (0-1)
    pub context_weight: f32,
    /// Custom metadata filters
    pub metadata_filters: HashMap<String, String>,
}

impl QueryContext {
    /// Create a new empty context
    pub fn new() -> Self {
        Self {
            recency_weight: 0.2,
            frequency_weight: 0.1,
            context_weight: 0.2,
            ..Default::default()
        }
    }

    /// Set current project
    pub fn with_current_project(mut self, project: &str) -> Self {
        self.current_project = Some(project.to_string());
        self
    }

    /// Set recency boost weight
    pub fn with_recency_boost(mut self, weight: f32) -> Self {
        self.recency_weight = weight.clamp(0.0, 1.0);
        self
    }

    /// Add recent files for context
    pub fn with_recent_files(mut self, files: Vec<PathBuf>) -> Self {
        self.recent_files = files;
        self
    }
}

// =============================================================================
// Query Parser
// =============================================================================

/// Parses natural language queries into structured form
pub struct QueryParser {
    /// Intent keywords mapping
    intent_keywords: HashMap<&'static str, QueryIntent>,
    /// File type keywords
    file_type_keywords: HashMap<&'static str, FileType>,
    /// Time modifiers
    time_keywords: HashMap<&'static str, TimeRange>,
    /// Stop words to filter
    stop_words: Vec<&'static str>,
}

impl QueryParser {
    /// Create a new query parser
    pub fn new() -> Self {
        let mut intent_keywords = HashMap::new();
        intent_keywords.insert("find", QueryIntent::Find);
        intent_keywords.insert("search", QueryIntent::Search);
        intent_keywords.insert("locate", QueryIntent::Find);
        intent_keywords.insert("get", QueryIntent::Find);
        intent_keywords.insert("show", QueryIntent::List);
        intent_keywords.insert("list", QueryIntent::List);
        intent_keywords.insert("display", QueryIntent::List);
        intent_keywords.insert("compare", QueryIntent::Compare);
        intent_keywords.insert("diff", QueryIntent::Compare);
        intent_keywords.insert("similar", QueryIntent::Similar);
        intent_keywords.insert("like", QueryIntent::Similar);
        intent_keywords.insert("recent", QueryIntent::Recent);
        intent_keywords.insert("latest", QueryIntent::Recent);
        intent_keywords.insert("new", QueryIntent::Recent);
        intent_keywords.insert("by", QueryIntent::ByAuthor);

        let mut file_type_keywords = HashMap::new();
        file_type_keywords.insert("code", FileType::Code);
        file_type_keywords.insert("script", FileType::Code);
        file_type_keywords.insert("program", FileType::Code);
        file_type_keywords.insert("source", FileType::Code);
        file_type_keywords.insert("text", FileType::Text);
        file_type_keywords.insert("document", FileType::Text);
        file_type_keywords.insert("doc", FileType::Text);
        file_type_keywords.insert("note", FileType::Text);
        file_type_keywords.insert("image", FileType::Image);
        file_type_keywords.insert("picture", FileType::Image);
        file_type_keywords.insert("photo", FileType::Image);
        file_type_keywords.insert("audio", FileType::Audio);
        file_type_keywords.insert("music", FileType::Audio);
        file_type_keywords.insert("sound", FileType::Audio);
        file_type_keywords.insert("video", FileType::Video);
        file_type_keywords.insert("movie", FileType::Video);

        let mut time_keywords = HashMap::new();
        time_keywords.insert("today", TimeRange::last_hours(24));
        time_keywords.insert("yesterday", TimeRange::last_days(2));
        time_keywords.insert("week", TimeRange::this_week());
        time_keywords.insert("month", TimeRange::this_month());
        time_keywords.insert("recent", TimeRange::last_days(7));
        time_keywords.insert("lately", TimeRange::last_days(7));

        let stop_words = vec![
            "the", "a", "an", "and", "or", "but", "in", "on", "at", "to", "for",
            "of", "with", "about", "from", "that", "this", "these", "those",
            "is", "are", "was", "were", "be", "been", "being",
            "have", "has", "had", "do", "does", "did", "will", "would", "could", "should",
            "me", "my", "i", "you", "your", "we", "our", "they", "their",
        ];

        Self {
            intent_keywords,
            file_type_keywords,
            time_keywords,
            stop_words,
        }
    }

    /// Parse a natural language query
    pub fn parse(&self, query: &str) -> ParsedQuery {
        let lower = query.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();

        let mut parsed = ParsedQuery {
            original: query.to_string(),
            intent: QueryIntent::Search,
            core_terms: Vec::new(),
            expanded_terms: Vec::new(),
            file_type: None,
            time_range: None,
            path_filter: None,
            tags: Vec::new(),
            recency_boost: false,
            reference_file: None,
        };

        // Detect intent
        for word in &words {
            if let Some(intent) = self.intent_keywords.get(*word) {
                parsed.intent = *intent;
                break;
            }
        }

        // Detect file type
        for word in &words {
            if let Some(file_type) = self.file_type_keywords.get(*word) {
                parsed.file_type = Some(*file_type);
                break;
            }
        }

        // Detect time range
        for word in &words {
            if let Some(time_range) = self.time_keywords.get(*word) {
                parsed.time_range = Some(*time_range);
                parsed.recency_boost = true;
                break;
            }
        }

        // Check for "last X days/hours" pattern
        for i in 0..words.len().saturating_sub(2) {
            if words[i] == "last" {
                if let Ok(n) = words[i + 1].parse::<u64>() {
                    let unit = words.get(i + 2).copied().unwrap_or("");
                    if unit.starts_with("day") {
                        parsed.time_range = Some(TimeRange::last_days(n));
                        parsed.recency_boost = true;
                    } else if unit.starts_with("hour") {
                        parsed.time_range = Some(TimeRange::last_hours(n));
                        parsed.recency_boost = true;
                    } else if unit.starts_with("week") {
                        parsed.time_range = Some(TimeRange::last_days(n * 7));
                        parsed.recency_boost = true;
                    }
                }
            }
        }

        // Check for path filter (in/from directory)
        for i in 0..words.len().saturating_sub(1) {
            if (words[i] == "in" || words[i] == "from") && words[i + 1].contains('/') {
                parsed.path_filter = Some(words[i + 1].to_string());
            }
        }

        // Extract core terms (filter out modifiers, intents, and stop words)
        let modifier_words: Vec<&str> = self.intent_keywords.keys().copied()
            .chain(self.file_type_keywords.keys().copied())
            .chain(self.time_keywords.keys().copied())
            .collect();

        for word in &words {
            if self.stop_words.contains(word) {
                continue;
            }
            if modifier_words.contains(word) {
                continue;
            }
            // Skip numbers that are part of time expressions
            if word.parse::<u64>().is_ok() {
                continue;
            }
            parsed.core_terms.push((*word).to_string());
        }

        // Handle specific language keywords (python, rust, javascript, etc.)
        let language_keywords = [
            ("python", FileType::Code),
            ("rust", FileType::Code),
            ("javascript", FileType::Code),
            ("typescript", FileType::Code),
            ("java", FileType::Code),
            ("go", FileType::Code),
            ("c++", FileType::Code),
            ("cpp", FileType::Code),
            ("ruby", FileType::Code),
        ];

        for (lang, file_type) in &language_keywords {
            if words.contains(lang) {
                parsed.file_type = Some(*file_type);
                // Keep language as a core term for embedding
            }
        }

        parsed
    }
}

impl Default for QueryParser {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Query Expander
// =============================================================================

/// Expands queries with synonyms and related terms
pub struct QueryExpander {
    /// Synonym mappings
    synonyms: HashMap<&'static str, Vec<&'static str>>,
    /// Technical term expansions
    tech_expansions: HashMap<&'static str, Vec<&'static str>>,
}

impl QueryExpander {
    /// Create a new query expander
    pub fn new() -> Self {
        let mut synonyms = HashMap::new();

        // Common synonyms
        synonyms.insert("find", vec!["search", "locate", "get"]);
        synonyms.insert("error", vec!["bug", "issue", "problem", "fault"]);
        synonyms.insert("fix", vec!["repair", "solve", "resolve", "patch"]);
        synonyms.insert("config", vec!["configuration", "settings", "options"]);
        synonyms.insert("docs", vec!["documentation", "manual", "guide"]);
        synonyms.insert("test", vec!["spec", "unittest", "integration"]);
        synonyms.insert("api", vec!["interface", "endpoint", "service"]);
        synonyms.insert("db", vec!["database", "storage", "datastore"]);
        synonyms.insert("auth", vec!["authentication", "login", "credentials"]);
        synonyms.insert("ui", vec!["interface", "frontend", "view"]);

        let mut tech_expansions = HashMap::new();

        // Technical domain expansions
        tech_expansions.insert("ml", vec!["machine learning", "model", "training", "inference"]);
        tech_expansions.insert("machine learning", vec!["ml", "deep learning", "neural network", "ai"]);
        tech_expansions.insert("ai", vec!["artificial intelligence", "machine learning", "ml"]);
        tech_expansions.insert("deep learning", vec!["neural network", "transformer", "cnn", "rnn"]);
        tech_expansions.insert("web", vec!["http", "html", "css", "frontend", "backend"]);
        tech_expansions.insert("frontend", vec!["ui", "react", "vue", "angular", "client"]);
        tech_expansions.insert("backend", vec!["server", "api", "database", "service"]);
        tech_expansions.insert("devops", vec!["ci", "cd", "deployment", "infrastructure"]);
        tech_expansions.insert("security", vec!["auth", "encryption", "vulnerability", "ssl"]);
        tech_expansions.insert("performance", vec!["optimization", "speed", "latency", "throughput"]);

        Self {
            synonyms,
            tech_expansions,
        }
    }

    /// Expand a parsed query with related terms
    pub fn expand(&self, query: &mut ParsedQuery) {
        let mut expanded = Vec::new();

        for term in &query.core_terms {
            let term_lower = term.to_lowercase();

            // Add synonyms
            if let Some(syns) = self.synonyms.get(term_lower.as_str()) {
                for syn in syns {
                    if !expanded.contains(&syn.to_string()) {
                        expanded.push(syn.to_string());
                    }
                }
            }

            // Add technical expansions
            if let Some(exps) = self.tech_expansions.get(term_lower.as_str()) {
                for exp in exps {
                    if !expanded.contains(&exp.to_string()) {
                        expanded.push(exp.to_string());
                    }
                }
            }
        }

        query.expanded_terms = expanded;
    }

    /// Get the full query string (core + expanded)
    pub fn get_full_query(&self, query: &ParsedQuery) -> String {
        let mut terms: Vec<&str> = query.core_terms.iter().map(|s| s.as_str()).collect();

        // Add some expanded terms (but not too many to avoid noise)
        let max_expanded = 3.min(query.expanded_terms.len());
        for term in query.expanded_terms.iter().take(max_expanded) {
            terms.push(term);
        }

        terms.join(" ")
    }
}

impl Default for QueryExpander {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Multi-Factor Ranker
// =============================================================================

/// Ranking factors and their weights
#[derive(Debug, Clone)]
pub struct RankingWeights {
    /// Weight for semantic similarity (0-1)
    pub similarity: f32,
    /// Weight for recency (0-1)
    pub recency: f32,
    /// Weight for access frequency (0-1)
    pub frequency: f32,
    /// Weight for context relevance (0-1)
    pub context: f32,
    /// Weight for file type match (0-1)
    pub file_type_match: f32,
}

impl Default for RankingWeights {
    fn default() -> Self {
        Self {
            similarity: 0.5,
            recency: 0.2,
            frequency: 0.1,
            context: 0.15,
            file_type_match: 0.05,
        }
    }
}

/// Computed ranking score with breakdown
#[derive(Debug, Clone)]
pub struct RankingScore {
    /// Total score (weighted combination)
    pub total: f32,
    /// Semantic similarity score
    pub similarity: f32,
    /// Recency score
    pub recency: f32,
    /// Access frequency score
    pub frequency: f32,
    /// Context relevance score
    pub context: f32,
    /// File type match bonus
    pub file_type_match: f32,
}

/// Ranks search results using multiple factors
pub struct MultiFactorRanker {
    weights: RankingWeights,
}

impl MultiFactorRanker {
    /// Create a new ranker with default weights
    pub fn new() -> Self {
        Self {
            weights: RankingWeights::default(),
        }
    }

    /// Create a ranker with custom weights
    pub fn with_weights(weights: RankingWeights) -> Self {
        Self { weights }
    }

    /// Rank a list of search results
    pub fn rank(
        &self,
        results: Vec<SearchResult>,
        query: &ParsedQuery,
        context: &QueryContext,
    ) -> Vec<(SearchResult, RankingScore)> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let mut scored: Vec<(SearchResult, RankingScore)> = results
            .into_iter()
            .map(|result| {
                let score = self.compute_score(&result, query, context, now);
                (result, score)
            })
            .collect();

        // Sort by total score descending
        scored.sort_by(|a, b| {
            b.1.total
                .partial_cmp(&a.1.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        scored
    }

    /// Compute the ranking score for a single result
    fn compute_score(
        &self,
        result: &SearchResult,
        query: &ParsedQuery,
        context: &QueryContext,
        now: u64,
    ) -> RankingScore {
        // 1. Semantic similarity (already computed by HNSW)
        let similarity = result.similarity;

        // 2. Recency score (exponential decay)
        let age_days = (now - result.document.metadata.modified) as f32 / 86400.0;
        let recency = (-age_days / 30.0).exp(); // Half-life of ~30 days

        // 3. Access frequency (placeholder - would need access log)
        let frequency = 0.5; // Default to medium

        // 4. Context relevance
        let mut context_score: f32 = 0.0;

        // Boost if in current project
        if let Some(ref project) = context.current_project {
            let path_str = result.document.metadata.path.to_string_lossy();
            if path_str.contains(project) {
                context_score += 0.5;
            }
        }

        // Boost if similar to recently accessed files
        for recent in &context.recent_files {
            if result.document.metadata.path.parent() == recent.parent() {
                context_score += 0.3;
                break;
            }
        }

        context_score = context_score.min(1.0);

        // 5. File type match
        let file_type_match = match query.file_type {
            Some(ft) if result.document.metadata.file_type == ft => 1.0,
            Some(_) => 0.0,
            None => 0.5, // Neutral if no filter
        };

        // Apply time range filter (hard filter, not a score)
        let in_time_range = match query.time_range {
            Some(range) => {
                let modified = result.document.metadata.modified;
                modified >= range.start && modified <= range.end
            }
            None => true,
        };

        // Compute weighted total
        let total = if in_time_range {
            similarity * self.weights.similarity
                + recency * self.weights.recency
                + frequency * self.weights.frequency
                + context_score * self.weights.context
                + file_type_match * self.weights.file_type_match
        } else {
            0.0 // Exclude if outside time range
        };

        RankingScore {
            total,
            similarity,
            recency,
            frequency,
            context: context_score,
            file_type_match,
        }
    }
}

impl Default for MultiFactorRanker {
    fn default() -> Self {
        Self::new()
    }
}

// =============================================================================
// Result Presentation
// =============================================================================

/// A formatted query result
#[derive(Debug, Clone)]
pub struct FormattedResult {
    /// File path
    pub path: PathBuf,
    /// Confidence score (0-1)
    pub confidence: f32,
    /// Human-readable explanation
    pub explanation: String,
    /// Content snippet/preview
    pub snippet: String,
    /// Ranking breakdown
    pub ranking: RankingScore,
    /// Original document metadata
    pub file_type: FileType,
    /// Last modified timestamp
    pub modified: u64,
    /// Matching tags
    pub tags: Vec<String>,
}

/// The complete query response
#[derive(Debug, Clone)]
pub struct QueryResponse {
    /// Original query
    pub query: String,
    /// Parsed query (for debugging/transparency)
    pub parsed_query: ParsedQuery,
    /// Formatted results
    pub items: Vec<FormattedResult>,
    /// Total number of matches (before limit)
    pub total_matches: usize,
    /// Query execution time
    pub execution_time_ms: u64,
    /// Suggestions for query refinement
    pub suggestions: Vec<String>,
}

/// Formats results for presentation
pub struct ResultPresenter;

impl ResultPresenter {
    /// Format ranked results for presentation
    pub fn format(
        ranked: Vec<(SearchResult, RankingScore)>,
        query: &ParsedQuery,
        execution_time_ms: u64,
    ) -> QueryResponse {
        let total_matches = ranked.len();

        let items: Vec<FormattedResult> = ranked
            .into_iter()
            .map(|(result, score)| {
                let explanation = Self::generate_explanation(&result, &score, query);

                FormattedResult {
                    path: result.document.metadata.path.clone(),
                    confidence: score.total,
                    explanation,
                    snippet: result.document.content_preview.clone(),
                    ranking: score,
                    file_type: result.document.metadata.file_type,
                    modified: result.document.metadata.modified,
                    tags: result.document.metadata.tags.clone(),
                }
            })
            .collect();

        // Generate suggestions
        let suggestions = Self::generate_suggestions(query, &items);

        QueryResponse {
            query: query.original.clone(),
            parsed_query: query.clone(),
            items,
            total_matches,
            execution_time_ms,
            suggestions,
        }
    }

    /// Generate a human-readable explanation for why this result matched
    fn generate_explanation(
        result: &SearchResult,
        score: &RankingScore,
        query: &ParsedQuery,
    ) -> String {
        let mut reasons = Vec::new();

        if score.similarity > 0.7 {
            reasons.push("strong semantic match".to_string());
        } else if score.similarity > 0.5 {
            reasons.push("moderate semantic match".to_string());
        }

        if score.recency > 0.8 {
            reasons.push("recently modified".to_string());
        }

        if score.context > 0.5 {
            reasons.push("relevant to current context".to_string());
        }

        if score.file_type_match > 0.9 {
            reasons.push(format!("matches {:?} filter", query.file_type.unwrap_or(FileType::Unknown)));
        }

        // Check for term matches in preview
        let preview_lower = result.document.content_preview.to_lowercase();
        let matching_terms: Vec<String> = query.core_terms
            .iter()
            .filter(|t| preview_lower.contains(&t.to_lowercase()))
            .cloned()
            .collect();

        if !matching_terms.is_empty() {
            reasons.push(format!("contains: {}", matching_terms.iter().take(3).cloned().collect::<Vec<_>>().join(", ")));
        }

        if reasons.is_empty() {
            reasons.push("potential match".to_string());
        }

        reasons.join("; ")
    }

    /// Generate suggestions for refining the query
    fn generate_suggestions(query: &ParsedQuery, results: &[FormattedResult]) -> Vec<String> {
        let mut suggestions = Vec::new();

        // Suggest file type filter if not specified
        if query.file_type.is_none() && !results.is_empty() {
            let mut type_counts: HashMap<FileType, usize> = HashMap::new();
            for r in results {
                *type_counts.entry(r.file_type).or_insert(0) += 1;
            }

            if let Some((dominant_type, count)) = type_counts.iter().max_by_key(|(_, c)| *c) {
                if *count > results.len() / 2 {
                    suggestions.push(format!(
                        "Try: \"{}\" with {:?} filter",
                        query.original,
                        dominant_type
                    ));
                }
            }
        }

        // Suggest time filter if results span wide time range
        if query.time_range.is_none() && results.len() > 5 {
            suggestions.push(format!(
                "Try: \"{}\" from last week",
                query.original
            ));
        }

        // Suggest expanded terms
        if !query.expanded_terms.is_empty() && results.len() < 3 {
            let expanded = query.expanded_terms.first().cloned().unwrap_or_default();
            suggestions.push(format!(
                "Try searching for \"{}\" instead",
                expanded
            ));
        }

        suggestions
    }
}

// =============================================================================
// Semantic Query Engine (Main Interface)
// =============================================================================

/// The main semantic query engine
pub struct SemanticQueryEngine {
    /// Semantic file system
    fs: Arc<SemanticFS>,
    /// Query parser
    parser: QueryParser,
    /// Query expander
    expander: QueryExpander,
    /// Result ranker
    ranker: MultiFactorRanker,
}

impl SemanticQueryEngine {
    /// Create a new query engine
    pub fn new(fs: Arc<SemanticFS>) -> Self {
        Self {
            fs,
            parser: QueryParser::new(),
            expander: QueryExpander::new(),
            ranker: MultiFactorRanker::new(),
        }
    }

    /// Execute a natural language query
    pub async fn query(&self, query: &str) -> Result<QueryResponse> {
        self.query_with_context(query, QueryContext::default()).await
    }

    /// Execute a query with context
    pub async fn query_with_context(
        &self,
        query: &str,
        context: QueryContext,
    ) -> Result<QueryResponse> {
        let start = std::time::Instant::now();

        // 1. Parse the query
        let mut parsed = self.parser.parse(query);

        // 2. Expand with synonyms/related terms
        self.expander.expand(&mut parsed);

        // 3. Build the search query
        let full_query = self.expander.get_full_query(&parsed);

        log::debug!(
            "Query: '{}' -> Intent: {:?}, Core: {:?}, Expanded: {:?}",
            query,
            parsed.intent,
            parsed.core_terms,
            parsed.expanded_terms
        );

        // 4. Execute vector search
        let limit = 50; // Get more results for ranking
        let results = self.fs.search(&full_query, limit).await?;

        // 5. Rank results
        let ranked = self.ranker.rank(results, &parsed, &context);

        // 6. Format response
        let execution_time_ms = start.elapsed().as_millis() as u64;
        let response = ResultPresenter::format(ranked, &parsed, execution_time_ms);

        Ok(response)
    }

    /// Find files similar to a given file
    pub async fn find_similar(&self, path: &PathBuf, limit: usize) -> Result<QueryResponse> {
        let start = std::time::Instant::now();

        // Create a parsed query for similar files
        let parsed = ParsedQuery {
            original: format!("similar to {}", path.display()),
            intent: QueryIntent::Similar,
            core_terms: vec![],
            expanded_terms: vec![],
            file_type: None,
            time_range: None,
            path_filter: None,
            tags: vec![],
            recency_boost: false,
            reference_file: Some(path.clone()),
        };

        // Read the reference file to use as query
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read {}", path.display()))?;

        // Use first 500 chars as query
        let query_text = if content.len() > 500 {
            &content[..500]
        } else {
            &content
        };

        let results = self.fs.search(query_text, limit + 1).await?;

        // Remove the reference file itself
        let filtered: Vec<_> = results
            .into_iter()
            .filter(|r| r.document.metadata.path != *path)
            .take(limit)
            .collect();

        let ranked: Vec<_> = filtered
            .into_iter()
            .map(|r| {
                let score = RankingScore {
                    total: r.similarity,
                    similarity: r.similarity,
                    recency: 0.5,
                    frequency: 0.5,
                    context: 0.5,
                    file_type_match: 0.5,
                };
                (r, score)
            })
            .collect();

        let execution_time_ms = start.elapsed().as_millis() as u64;
        let response = ResultPresenter::format(ranked, &parsed, execution_time_ms);

        Ok(response)
    }

    /// Get recent files (convenience method)
    pub async fn recent_files(&self, limit: usize) -> Result<QueryResponse> {
        self.query_with_context(
            "recent files",
            QueryContext::new().with_recency_boost(0.8),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_parser_intent() {
        let parser = QueryParser::new();

        let q1 = parser.parse("find python scripts about data processing");
        assert_eq!(q1.intent, QueryIntent::Find);
        assert_eq!(q1.file_type, Some(FileType::Code)); // "script" maps to Code

        let q2 = parser.parse("show me recent doc files");
        assert_eq!(q2.intent, QueryIntent::List);
        assert_eq!(q2.file_type, Some(FileType::Text)); // "doc" maps to Text
        assert!(q2.recency_boost);

        let q3 = parser.parse("files similar to this one");
        assert_eq!(q3.intent, QueryIntent::Similar);
    }

    #[test]
    fn test_query_parser_time_range() {
        let parser = QueryParser::new();

        let q1 = parser.parse("files from last 3 days");
        assert!(q1.time_range.is_some());
        let range = q1.time_range.unwrap();
        let expected_duration: i64 = 3 * 24 * 60 * 60;
        assert!(((range.end - range.start) as i64 - expected_duration).abs() < 60);

        let q2 = parser.parse("changes from this week");
        assert!(q2.time_range.is_some());
    }

    #[test]
    fn test_query_expander() {
        let expander = QueryExpander::new();
        let mut query = ParsedQuery::simple("ml config error");

        expander.expand(&mut query);

        assert!(!query.expanded_terms.is_empty());
        // "ml" should expand to include "machine learning", "error" to "bug"
        assert!(query.expanded_terms.iter().any(|t| t.contains("machine") || t.contains("bug")));
    }

    #[test]
    fn test_time_range() {
        let range = TimeRange::last_days(7);
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        assert!(range.end <= now);
        assert!(range.end - range.start >= 7 * 24 * 60 * 60 - 1);
    }

    #[test]
    fn test_query_context_builder() {
        let ctx = QueryContext::new()
            .with_current_project("my-project")
            .with_recency_boost(0.5);

        assert_eq!(ctx.current_project, Some("my-project".to_string()));
        assert_eq!(ctx.recency_weight, 0.5);
    }
}
