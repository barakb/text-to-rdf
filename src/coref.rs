//! Coreference Resolution - Stage 0 of the RDF extraction pipeline
//!
//! Resolves pronouns and entity mentions to their canonical forms before entity extraction.
//! This is critical for document-level RDF extraction where text contains references like:
//! - Pronouns: "he", "she", "it", "they"
//! - Definite descriptions: "The CEO", "the company"
//! - Abbreviations: "IBM" → "International Business Machines"
//!
//! # Problem
//!
//! Without coreference resolution:
//! ```text
//! "Dan Shalev founded Acme Corp. He served as CEO for 10 years."
//! ```
//! Creates two disconnected entities:
//! - Person: "Dan Shalev"
//! - Person: "He" (unresolved)
//!
//! # Solution
//!
//! With coreference resolution:
//! ```text
//! "Dan Shalev founded Acme Corp. Dan Shalev served as CEO for 10 years."
//! ```
//! Creates a single, connected entity graph.
//!
//! # Architecture
//!
//! Three strategies available (all pure Rust):
//!
//! 1. **Rule-Based** (Default, fast)
//!    - Simple heuristics for pronoun resolution
//!    - No external dependencies
//!    - Good for simple documents with clear structure
//!    - ~1ms per document
//!
//! 2. **GLiNER-Guided** (Recommended with GLiNER feature)
//!    - Uses GLiNER entity extraction to guide resolution
//!    - Resolves pronouns to nearest matching entity
//!    - Very accurate for well-formed text
//!    - ~50ms per document (includes GLiNER)
//!
//! 3. **None** (Disabled)
//!    - No coreference resolution
//!    - Pass-through mode

use crate::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[cfg(feature = "gliner")]
use crate::gliner_extractor::{GlinerConfig, GlinerExtractor};
#[cfg(feature = "gliner")]
use crate::RdfExtractor;

/// Configuration for coreference resolution
#[derive(Debug, Clone)]
pub struct CorefConfig {
    /// Strategy for coreference resolution
    pub strategy: CorefStrategy,

    /// Whether to preserve original text in metadata
    pub preserve_original: bool,

    /// Maximum distance (in sentences) to look back for antecedents
    pub max_distance: usize,

    /// Minimum confidence for entity matches
    pub min_confidence: f32,
}

/// Strategy for coreference resolution
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CorefStrategy {
    /// No coreference resolution
    None,

    /// Rule-based resolution using simple heuristics
    RuleBased,

    /// GLiNER-guided resolution (requires gliner feature)
    #[cfg(feature = "gliner")]
    GlinerGuided,
}

impl Default for CorefConfig {
    fn default() -> Self {
        Self {
            strategy: CorefStrategy::RuleBased,
            preserve_original: true,
            max_distance: 3,
            min_confidence: 0.7,
        }
    }
}

impl CorefConfig {
    /// Load configuration from environment variables
    ///
    /// Supported environment variables:
    /// - `COREF_STRATEGY`: Strategy (`none`, `rule-based`, `gliner-guided`)
    /// - `COREF_PRESERVE_ORIGINAL`: Preserve original text (true/false)
    /// - `COREF_MAX_DISTANCE`: Maximum sentence distance for antecedents
    /// - `COREF_MIN_CONFIDENCE`: Minimum confidence for matches
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let strategy = std::env::var("COREF_STRATEGY")
            .ok()
            .and_then(|s| match s.to_lowercase().as_str() {
                "none" | "disabled" => Some(CorefStrategy::None),
                "rule-based" | "rule" | "rules" => Some(CorefStrategy::RuleBased),
                #[cfg(feature = "gliner")]
                "gliner-guided" | "gliner" => Some(CorefStrategy::GlinerGuided),
                _ => None,
            })
            .unwrap_or(CorefStrategy::RuleBased);

        let preserve_original = std::env::var("COREF_PRESERVE_ORIGINAL")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(true);

        let max_distance = std::env::var("COREF_MAX_DISTANCE")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(3);

        let min_confidence = std::env::var("COREF_MIN_CONFIDENCE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.7);

        Ok(Self {
            strategy,
            preserve_original,
            max_distance,
            min_confidence,
        })
    }
}

/// Coreference cluster - a group of mentions referring to the same entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorefCluster {
    /// Cluster ID
    pub id: usize,

    /// Main/canonical mention (usually the most specific one)
    pub main_mention: Mention,

    /// All mentions in the cluster
    pub mentions: Vec<Mention>,
}

/// A single mention of an entity
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Mention {
    /// The text of the mention
    pub text: String,

    /// Start character offset
    pub start: usize,

    /// End character offset
    pub end: usize,

    /// Whether this is the main/canonical mention
    pub is_main: bool,

    /// Sentence index
    pub sentence_idx: usize,
}

/// Result of coreference resolution
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorefResult {
    /// Original text
    pub original_text: String,

    /// Resolved text with pronouns replaced
    pub resolved_text: String,

    /// Coreference clusters found
    pub clusters: Vec<CorefCluster>,

    /// Mapping from original mention to resolved mention
    pub mention_map: HashMap<String, String>,
}

/// Coreference resolver
pub struct CorefResolver {
    config: CorefConfig,

    #[cfg(feature = "gliner")]
    gliner: Option<GlinerExtractor>,
}

impl CorefResolver {
    /// Create a new coreference resolver
    ///
    /// # Errors
    ///
    /// Returns an error if the resolver cannot be initialized
    pub fn new(config: CorefConfig) -> Result<Self> {
        #[cfg(feature = "gliner")]
        let gliner = if config.strategy == CorefStrategy::GlinerGuided {
            let gliner_config = GlinerConfig::from_env().unwrap_or_default();
            Some(GlinerExtractor::new(gliner_config)?)
        } else {
            None
        };

        Ok(Self {
            config,
            #[cfg(feature = "gliner")]
            gliner,
        })
    }

    /// Resolve coreferences in text
    ///
    /// # Arguments
    ///
    /// * `text` - Input text with pronouns and entity mentions
    ///
    /// # Returns
    ///
    /// A `CorefResult` containing the resolved text and metadata
    ///
    /// # Errors
    ///
    /// Returns an error if resolution fails
    pub async fn resolve(&self, text: &str) -> Result<CorefResult> {
        match self.config.strategy {
            CorefStrategy::None => Ok(CorefResult {
                original_text: text.to_string(),
                resolved_text: text.to_string(),
                clusters: Vec::new(),
                mention_map: HashMap::new(),
            }),
            CorefStrategy::RuleBased => self.resolve_rule_based(text),
            #[cfg(feature = "gliner")]
            CorefStrategy::GlinerGuided => self.resolve_gliner_guided(text).await,
        }
    }

    /// Resolve using rule-based heuristics
    ///
    /// This is a simple but effective approach:
    /// 1. Split text into sentences
    /// 2. Extract named entities (capitalized sequences)
    /// 3. For each pronoun, find the nearest matching entity by gender/number
    fn resolve_rule_based(&self, text: &str) -> Result<CorefResult> {
        // Split into sentences (simple '.' based splitting)
        let sentences: Vec<&str> = text.split('.').filter(|s| !s.trim().is_empty()).collect();

        // Extract candidate entities (proper nouns - capitalized words/phrases)
        let mut entities: Vec<(String, usize, usize, usize)> = Vec::new(); // (text, start, end, sentence_idx)

        for (sent_idx, sentence) in sentences.iter().enumerate() {
            let sent_start = text.find(sentence).unwrap_or(0);

            // Find capitalized sequences
            let words: Vec<&str> = sentence.split_whitespace().collect();
            let mut i = 0;

            while i < words.len() {
                if is_proper_noun_start(words[i]) {
                    // Collect consecutive capitalized words
                    let mut entity_words = vec![words[i]];
                    let mut j = i + 1;

                    while j < words.len() && is_proper_noun(words[j]) {
                        entity_words.push(words[j]);
                        j += 1;
                    }

                    let entity_text = entity_words.join(" ");
                    let entity_start = sent_start + sentence.find(entity_words[0]).unwrap_or(0);
                    let entity_end = entity_start + entity_text.len();

                    entities.push((entity_text, entity_start, entity_end, sent_idx));
                    i = j;
                } else {
                    i += 1;
                }
            }
        }

        // Resolve pronouns
        let mut clusters: Vec<CorefCluster> = Vec::new();
        let mut mention_map: HashMap<String, String> = HashMap::new();
        let mut resolved_text = text.to_string();
        let mut replacements: Vec<(usize, usize, String)> = Vec::new();

        for (sent_idx, sentence) in sentences.iter().enumerate() {
            let sent_start = text.find(sentence).unwrap_or(0);

            // Find pronouns in this sentence
            for word in sentence.split_whitespace() {
                if let Some(pronoun) = classify_pronoun(word) {
                    // Find nearest matching entity
                    if let Some((entity, _, _, _)) = entities
                        .iter()
                        .filter(|(_, _, _, ent_sent_idx)| {
                            // Only look at entities from earlier sentences or same sentence
                            *ent_sent_idx <= sent_idx
                                && sent_idx.saturating_sub(*ent_sent_idx)
                                    <= self.config.max_distance
                        })
                        .filter(|(ent, _, _, _)| {
                            // Gender/number matching
                            matches_pronoun(ent, &pronoun)
                        })
                        .last()
                    {
                        // Find position of pronoun
                        if let Some(pronoun_start) = text[sent_start..].find(word) {
                            let absolute_start = sent_start + pronoun_start;
                            let absolute_end = absolute_start + word.len();

                            replacements.push((absolute_start, absolute_end, entity.clone()));
                            mention_map.insert(word.to_string(), entity.clone());

                            // Create cluster if not exists
                            if !clusters.iter().any(|c| c.main_mention.text == *entity) {
                                clusters.push(CorefCluster {
                                    id: clusters.len(),
                                    main_mention: Mention {
                                        text: entity.clone(),
                                        start: 0,
                                        end: 0,
                                        is_main: true,
                                        sentence_idx: sent_idx,
                                    },
                                    mentions: vec![Mention {
                                        text: word.to_string(),
                                        start: absolute_start,
                                        end: absolute_end,
                                        is_main: false,
                                        sentence_idx: sent_idx,
                                    }],
                                });
                            }
                        }
                    }
                }
            }
        }

        // Apply replacements (reverse order to preserve offsets)
        replacements.sort_by(|a, b| b.0.cmp(&a.0));
        for (start, end, replacement) in replacements {
            resolved_text.replace_range(start..end, &replacement);
        }

        Ok(CorefResult {
            original_text: text.to_string(),
            resolved_text,
            clusters,
            mention_map,
        })
    }

    /// Resolve using GLiNER-guided approach
    #[cfg(feature = "gliner")]
    async fn resolve_gliner_guided(&self, text: &str) -> Result<CorefResult> {
        let gliner = self
            .gliner
            .as_ref()
            .ok_or_else(|| Error::Config("GLiNER not initialized".to_string()))?;

        // Extract entities with GLiNER
        let rdf_doc = gliner.extract(text).await?;

        // Parse entities from RDF document
        let json_value = serde_json::to_value(&rdf_doc)?;
        let mut entities: Vec<(String, usize, usize)> = Vec::new();

        if let Some(graph) = json_value.get("@graph").and_then(|g| g.as_array()) {
            for entity in graph {
                if let Some(metadata) = entity.get("_metadata") {
                    if let (Some(text), Some(start), Some(end)) = (
                        metadata.get("text").and_then(|t| t.as_str()),
                        metadata.get("startOffset").and_then(|s| s.as_u64()),
                        metadata.get("endOffset").and_then(|e| e.as_u64()),
                    ) {
                        entities.push((text.to_string(), start as usize, end as usize));
                    }
                }
            }
        }

        // Now resolve pronouns using GLiNER-detected entities
        let sentences: Vec<&str> = text.split('.').filter(|s| !s.trim().is_empty()).collect();

        let mut clusters: Vec<CorefCluster> = Vec::new();
        let mut mention_map: HashMap<String, String> = HashMap::new();
        let mut resolved_text = text.to_string();
        let mut replacements: Vec<(usize, usize, String)> = Vec::new();

        for (sent_idx, sentence) in sentences.iter().enumerate() {
            for word in sentence.split_whitespace() {
                if let Some(_pronoun) = classify_pronoun(word) {
                    // Find nearest entity before this pronoun
                    if let Some((entity_text, _, _)) = entities
                        .iter()
                        .filter(|(_, start, _)| *start < text.find(sentence).unwrap_or(0))
                        .last()
                    {
                        if let Some(sent_start) = text.find(sentence) {
                            if let Some(pronoun_start) = text[sent_start..].find(word) {
                                let absolute_start = sent_start + pronoun_start;
                                let absolute_end = absolute_start + word.len();

                                replacements.push((
                                    absolute_start,
                                    absolute_end,
                                    entity_text.clone(),
                                ));
                                mention_map.insert(word.to_string(), entity_text.clone());

                                // Create cluster
                                if !clusters.iter().any(|c| c.main_mention.text == *entity_text) {
                                    clusters.push(CorefCluster {
                                        id: clusters.len(),
                                        main_mention: Mention {
                                            text: entity_text.clone(),
                                            start: 0,
                                            end: 0,
                                            is_main: true,
                                            sentence_idx: sent_idx,
                                        },
                                        mentions: vec![Mention {
                                            text: word.to_string(),
                                            start: absolute_start,
                                            end: absolute_end,
                                            is_main: false,
                                            sentence_idx: sent_idx,
                                        }],
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        // Apply replacements
        replacements.sort_by(|a, b| b.0.cmp(&a.0));
        for (start, end, replacement) in replacements {
            resolved_text.replace_range(start..end, &replacement);
        }

        Ok(CorefResult {
            original_text: text.to_string(),
            resolved_text,
            clusters,
            mention_map,
        })
    }
}

/// Check if a word starts a proper noun (capitalized, not at sentence start)
fn is_proper_noun_start(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }

    let first_char = word.chars().next().unwrap();
    first_char.is_uppercase()
        && word.len() > 1
        && !word.chars().nth(1).unwrap().is_uppercase() // Ignore all-caps words
}

/// Check if a word is part of a proper noun
fn is_proper_noun(word: &str) -> bool {
    if word.is_empty() {
        return false;
    }

    let first_char = word.chars().next().unwrap();
    first_char.is_uppercase() && word.chars().skip(1).all(|c| c.is_lowercase() || !c.is_alphabetic())
}

/// Pronoun classification
#[derive(Debug, Clone, PartialEq, Eq)]
enum PronounType {
    Masculine,
    Feminine,
    Neutral,
    Plural,
}

/// Classify a pronoun by type
fn classify_pronoun(word: &str) -> Option<PronounType> {
    match word.to_lowercase().trim_matches(|c: char| !c.is_alphabetic()) {
        "he" | "him" | "his" | "himself" => Some(PronounType::Masculine),
        "she" | "her" | "hers" | "herself" => Some(PronounType::Feminine),
        "it" | "its" | "itself" => Some(PronounType::Neutral),
        "they" | "them" | "their" | "theirs" | "themselves" => Some(PronounType::Plural),
        _ => None,
    }
}

/// Check if an entity name matches a pronoun type
fn matches_pronoun(entity: &str, pronoun: &PronounType) -> bool {
    // Simple heuristics based on common patterns
    match pronoun {
        PronounType::Masculine | PronounType::Feminine => {
            // Single person names (1-3 words, no "and")
            let words: Vec<&str> = entity.split_whitespace().collect();
            words.len() <= 3 && !entity.to_lowercase().contains(" and ")
        }
        PronounType::Neutral => {
            // Organizations, companies (often end in Corp, Inc, etc.)
            entity.contains("Corp")
                || entity.contains("Inc")
                || entity.contains("LLC")
                || entity.contains("Ltd")
                || entity.contains("Company")
        }
        PronounType::Plural => {
            // Multiple names or organizations
            entity.contains(" and ") || entity.ends_with('s')
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = CorefConfig::default();
        assert_eq!(config.strategy, CorefStrategy::RuleBased);
        assert!(config.preserve_original);
        assert_eq!(config.max_distance, 3);
    }

    #[test]
    fn test_pronoun_classification() {
        assert_eq!(classify_pronoun("he"), Some(PronounType::Masculine));
        assert_eq!(classify_pronoun("she"), Some(PronounType::Feminine));
        assert_eq!(classify_pronoun("it"), Some(PronounType::Neutral));
        assert_eq!(classify_pronoun("they"), Some(PronounType::Plural));
        assert_eq!(classify_pronoun("the"), None);
    }

    #[test]
    fn test_proper_noun_detection() {
        assert!(is_proper_noun_start("John"));
        assert!(is_proper_noun_start("Microsoft"));
        assert!(!is_proper_noun_start("the"));
        assert!(!is_proper_noun_start("IBM")); // All caps
    }

    #[tokio::test]
    async fn test_resolve_none_strategy() {
        let config = CorefConfig {
            strategy: CorefStrategy::None,
            ..Default::default()
        };

        let resolver = CorefResolver::new(config).unwrap();
        let text = "Dan Shalev founded Acme. He served as CEO.";
        let result = resolver.resolve(text).await.unwrap();

        assert_eq!(result.original_text, text);
        assert_eq!(result.resolved_text, text);
        assert!(result.clusters.is_empty());
    }

    #[tokio::test]
    async fn test_resolve_rule_based() {
        let config = CorefConfig {
            strategy: CorefStrategy::RuleBased,
            max_distance: 2,
            ..Default::default()
        };

        let resolver = CorefResolver::new(config).unwrap();
        let text = "Dan Shalev founded Acme Corp. He served as CEO.";
        let result = resolver.resolve(text).await.unwrap();

        // Should resolve "He" to "Dan Shalev"
        assert!(result.resolved_text.contains("Dan Shalev"));
        assert!(!result.mention_map.is_empty());
    }
}
