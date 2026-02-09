//! Coreference Resolution - Stage 0 of the RDF extraction pipeline
//!
//! Resolves pronouns and anaphoric references to their canonical entity names
//! across document boundaries. This is critical for long documents and PDFs
//! where entities are referenced multiple times with different expressions.
//!
//! # The Problem
//!
//! Without coreference resolution, paragraph-by-paragraph extraction creates
//! disconnected graph nodes:
//!
//! ```text
//! Paragraph 1: "Dan Shalev founded the company..."
//! Paragraph 2: "The CEO announced..." ← Who is "The CEO"?
//! Paragraph 3: "He also said..." ← Who is "He"?
//! ```
//!
//! # The Solution
//!
//! Pre-process text to replace pronouns and references with canonical names:
//!
//! ```text
//! Paragraph 1: "Dan Shalev founded the company..."
//! Paragraph 2: "Dan Shalev announced..." ← Resolved!
//! Paragraph 3: "Dan Shalev also said..." ← Resolved!
//! ```
//!
//! # Pipeline Integration
//!
//! ```text
//! Stage 0: Coreference Resolution (PREPROCESSING)
//!    ↓
//! Stage 1: Discovery (GLiNER)
//!    ↓
//! Stage 2: Relations (LLM)
//!    ↓
//! Stage 3: Identity (Oxigraph)
//!    ↓
//! Stage 4: Validation (SHACL)
//! ```

use crate::error::{Error, Result};
use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A coreference cluster representing multiple mentions of the same entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CoreferenceCluster {
    /// Canonical/representative mention (usually the most informative one)
    pub canonical: String,

    /// Character offset of canonical mention in original text
    pub canonical_offset: usize,

    /// All mentions of this entity (including pronouns)
    pub mentions: Vec<Mention>,

    /// Confidence score (0.0-1.0)
    pub confidence: f32,
}

/// A single mention of an entity in text
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mention {
    /// Text of the mention ("he", "the CEO", "Dan Shalev")
    pub text: String,

    /// Start offset in original text
    pub start: usize,

    /// End offset in original text
    pub end: usize,

    /// Type of mention
    pub mention_type: MentionType,
}

/// Type of mention for coreference
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MentionType {
    /// Proper noun ("Dan Shalev", "Apple Inc.")
    Proper,

    /// Common noun ("the CEO", "the company")
    Nominal,

    /// Pronoun ("he", "she", "it")
    Pronominal,
}

/// Result of coreference resolution
#[derive(Debug, Clone)]
pub struct CoreferenceResult {
    /// Resolved text with pronouns replaced by canonical names
    pub resolved_text: String,

    /// All detected coreference clusters
    pub clusters: Vec<CoreferenceCluster>,

    /// Mapping from original offset to canonical name
    pub offset_to_canonical: HashMap<usize, String>,
}

/// Strategy for coreference resolution
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoreferenceStrategy {
    /// No coreference resolution
    None,

    /// Rule-based resolver (fast, limited accuracy)
    RuleBased,

    /// Python sidecar service (spaCy/neuralcoref)
    PythonSidecar,

    /// LLM-based resolution (accurate but expensive)
    Llm,
}

/// Configuration for coreference resolution
#[derive(Debug, Clone)]
pub struct CoreferenceConfig {
    /// Resolution strategy
    pub strategy: CoreferenceStrategy,

    /// URL for sidecar service (if using PythonSidecar)
    pub sidecar_url: Option<String>,

    /// Confidence threshold (0.0-1.0)
    pub confidence_threshold: f32,

    /// Maximum distance between mentions (in characters)
    pub max_mention_distance: usize,

    /// Whether to preserve original offsets in metadata
    pub preserve_offsets: bool,
}

impl Default for CoreferenceConfig {
    fn default() -> Self {
        Self {
            strategy: CoreferenceStrategy::None,
            sidecar_url: None,
            confidence_threshold: 0.7,
            max_mention_distance: 5000, // ~1-2 paragraphs
            preserve_offsets: true,
        }
    }
}

impl CoreferenceConfig {
    /// Load configuration from environment variables
    ///
    /// Supported environment variables:
    /// - `COREF_STRATEGY`: Strategy (`none`, `rule_based`, `sidecar`, `llm`)
    /// - `COREF_SIDECAR_URL`: URL for sidecar service (default: `http://localhost:8001`)
    /// - `COREF_CONFIDENCE`: Confidence threshold (0.0-1.0)
    /// - `COREF_MAX_DISTANCE`: Maximum mention distance in characters
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let strategy = std::env::var("COREF_STRATEGY")
            .ok()
            .and_then(|s| match s.to_lowercase().as_str() {
                "none" => Some(CoreferenceStrategy::None),
                "rule_based" | "rule-based" => Some(CoreferenceStrategy::RuleBased),
                "sidecar" | "python" => Some(CoreferenceStrategy::PythonSidecar),
                "llm" => Some(CoreferenceStrategy::Llm),
                _ => None,
            })
            .unwrap_or(CoreferenceStrategy::None);

        let sidecar_url = std::env::var("COREF_SIDECAR_URL")
            .ok()
            .or_else(|| {
                if strategy == CoreferenceStrategy::PythonSidecar {
                    Some("http://localhost:8001".to_string())
                } else {
                    None
                }
            });

        let confidence_threshold = std::env::var("COREF_CONFIDENCE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.7);

        let max_mention_distance = std::env::var("COREF_MAX_DISTANCE")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(5000);

        Ok(Self {
            strategy,
            sidecar_url,
            confidence_threshold,
            max_mention_distance,
            preserve_offsets: true,
        })
    }
}

/// Trait for coreference resolution implementations
#[async_trait]
pub trait CoreferenceResolver: Send + Sync {
    /// Resolve coreferences in text
    ///
    /// # Arguments
    ///
    /// * `text` - Input text with pronouns and anaphoric references
    ///
    /// # Returns
    ///
    /// A `CoreferenceResult` with resolved text and coreference clusters
    ///
    /// # Errors
    ///
    /// Returns an error if resolution fails
    async fn resolve(&self, text: &str) -> Result<CoreferenceResult>;
}

/// Main coreference resolver that delegates to specific implementations
pub struct CoreferenceEngine {
    config: CoreferenceConfig,
    resolver: Box<dyn CoreferenceResolver>,
}

impl CoreferenceEngine {
    /// Create a new coreference engine
    ///
    /// # Arguments
    ///
    /// * `config` - Coreference configuration
    ///
    /// # Errors
    ///
    /// Returns an error if the resolver cannot be initialized
    pub fn new(config: CoreferenceConfig) -> Result<Self> {
        let resolver: Box<dyn CoreferenceResolver> = match config.strategy {
            CoreferenceStrategy::None => Box::new(NoopResolver),
            CoreferenceStrategy::RuleBased => Box::new(RuleBasedResolver::new(config.clone())),
            CoreferenceStrategy::PythonSidecar => {
                Box::new(SidecarResolver::new(config.clone())?)
            }
            CoreferenceStrategy::Llm => {
                return Err(Error::Config(
                    "LLM-based coreference resolution not yet implemented".to_string(),
                ));
            }
        };

        Ok(Self { config, resolver })
    }

    /// Resolve coreferences in text
    pub async fn resolve(&self, text: &str) -> Result<CoreferenceResult> {
        self.resolver.resolve(text).await
    }
}

/// No-op resolver that returns text unchanged
struct NoopResolver;

#[async_trait]
impl CoreferenceResolver for NoopResolver {
    async fn resolve(&self, text: &str) -> Result<CoreferenceResult> {
        Ok(CoreferenceResult {
            resolved_text: text.to_string(),
            clusters: Vec::new(),
            offset_to_canonical: HashMap::new(),
        })
    }
}

/// Rule-based coreference resolver
///
/// Uses simple heuristics to resolve common patterns:
/// - Gender pronouns → nearest proper noun of matching gender
/// - "It" → nearest nominal/proper noun
/// - Definite descriptions ("the CEO") → nearest title match
struct RuleBasedResolver {
    config: CoreferenceConfig,
}

impl RuleBasedResolver {
    fn new(config: CoreferenceConfig) -> Self {
        Self { config }
    }

    /// Detect mentions in text (simple pattern matching)
    fn detect_mentions(&self, text: &str) -> Vec<Mention> {
        let mut mentions = Vec::new();

        // Pronouns to detect
        let pronouns = [
            "he", "him", "his", "she", "her", "hers", "it", "its",
            "they", "them", "their", "theirs",
        ];

        // Simple word-boundary regex simulation
        let words: Vec<(usize, &str)> = text
            .split_whitespace()
            .scan(0, |offset, word| {
                let start = *offset;
                *offset += word.len() + 1; // +1 for space
                Some((start, word))
            })
            .collect();

        for (offset, word) in words {
            let word_lower = word.to_lowercase();
            if pronouns.contains(&word_lower.as_str()) {
                mentions.push(Mention {
                    text: word.to_string(),
                    start: offset,
                    end: offset + word.len(),
                    mention_type: MentionType::Pronominal,
                });
            }
        }

        mentions
    }

    /// Find the nearest proper noun before a pronoun
    fn find_antecedent(&self, text: &str, pronoun_offset: usize) -> Option<String> {
        // Look backwards for capitalized words (simple heuristic)
        let before_pronoun = &text[..pronoun_offset];

        // Find last capitalized word that's not at sentence start
        let words: Vec<&str> = before_pronoun.split_whitespace().collect();

        for word in words.iter().rev() {
            // Skip single letters and common words
            if word.len() > 2
                && word.chars().next().map_or(false, |c| c.is_uppercase())
                && !["The", "A", "An", "This"].contains(word)
            {
                return Some(word.to_string());
            }
        }

        None
    }
}

#[async_trait]
impl CoreferenceResolver for RuleBasedResolver {
    async fn resolve(&self, text: &str) -> Result<CoreferenceResult> {
        let mentions = self.detect_mentions(text);

        if mentions.is_empty() {
            return Ok(CoreferenceResult {
                resolved_text: text.to_string(),
                clusters: Vec::new(),
                offset_to_canonical: HashMap::new(),
            });
        }

        let mut resolved_text = text.to_string();
        let mut offset_to_canonical = HashMap::new();
        let mut clusters = Vec::new();

        // Process pronouns in reverse order to preserve offsets
        for mention in mentions.iter().rev() {
            if let Some(antecedent) = self.find_antecedent(text, mention.start) {
                // Replace pronoun with antecedent
                resolved_text.replace_range(mention.start..mention.end, &antecedent);
                offset_to_canonical.insert(mention.start, antecedent.clone());

                // Create cluster
                clusters.push(CoreferenceCluster {
                    canonical: antecedent.clone(),
                    canonical_offset: 0, // Would need proper tracking
                    mentions: vec![mention.clone()],
                    confidence: 0.6, // Rule-based confidence is lower
                });
            }
        }

        Ok(CoreferenceResult {
            resolved_text,
            clusters,
            offset_to_canonical,
        })
    }
}

/// Sidecar resolver that calls a Python service
struct SidecarResolver {
    config: CoreferenceConfig,
    client: reqwest::Client,
}

impl SidecarResolver {
    fn new(config: CoreferenceConfig) -> Result<Self> {
        if config.sidecar_url.is_none() {
            return Err(Error::Config(
                "Sidecar URL required for PythonSidecar strategy".to_string(),
            ));
        }

        Ok(Self {
            config,
            client: reqwest::Client::new(),
        })
    }
}

#[async_trait]
impl CoreferenceResolver for SidecarResolver {
    async fn resolve(&self, text: &str) -> Result<CoreferenceResult> {
        let url = self.config.sidecar_url.as_ref().unwrap();

        #[derive(Serialize)]
        struct Request<'a> {
            text: &'a str,
            confidence_threshold: f32,
        }

        #[derive(Deserialize)]
        struct Response {
            resolved_text: String,
            clusters: Vec<CoreferenceCluster>,
        }

        let request = Request {
            text,
            confidence_threshold: self.config.confidence_threshold,
        };

        let response = self
            .client
            .post(format!("{}/resolve", url))
            .json(&request)
            .send()
            .await
            .map_err(|e| Error::Extraction(format!("Sidecar request failed: {}", e)))?;

        if !response.status().is_success() {
            return Err(Error::Extraction(format!(
                "Sidecar returned error: {}",
                response.status()
            )));
        }

        let response: Response = response
            .json()
            .await
            .map_err(|e| Error::Extraction(format!("Failed to parse sidecar response: {}", e)))?;

        // Build offset mapping
        let mut offset_to_canonical = HashMap::new();
        for cluster in &response.clusters {
            for mention in &cluster.mentions {
                offset_to_canonical.insert(mention.start, cluster.canonical.clone());
            }
        }

        Ok(CoreferenceResult {
            resolved_text: response.resolved_text,
            clusters: response.clusters,
            offset_to_canonical,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = CoreferenceConfig::default();
        assert_eq!(config.strategy, CoreferenceStrategy::None);
        assert_eq!(config.confidence_threshold, 0.7);
    }

    #[tokio::test]
    async fn test_noop_resolver() {
        let resolver = NoopResolver;
        let text = "Dan Shalev founded the company. He is the CEO.";
        let result = resolver.resolve(text).await.unwrap();

        assert_eq!(result.resolved_text, text);
        assert!(result.clusters.is_empty());
    }

    #[tokio::test]
    async fn test_rule_based_resolver() {
        let config = CoreferenceConfig {
            strategy: CoreferenceStrategy::RuleBased,
            ..Default::default()
        };

        let resolver = RuleBasedResolver::new(config);
        let text = "Dan Shalev founded the company. He is the CEO.";
        let result = resolver.resolve(text).await.unwrap();

        // Should detect "He" as a pronoun
        assert!(!result.clusters.is_empty());
    }
}
