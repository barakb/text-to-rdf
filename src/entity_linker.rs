//! Entity Linking Module - Stage 3 of the RDF extraction pipeline
//!
//! Links extracted entity names to canonical URIs from knowledge bases.
//! Implements "The URI Bridge" with:
//! - Fuzzy matching using Levenshtein distance for label alignment
//! - LLM-based disambiguation when multiple candidates exist
//! - Local Oxigraph store for offline operation
//!
//! Supports both remote APIs (DBpedia, Wikidata) and local Rust-native linking
//! via Oxigraph for production deployments.

use crate::error::{Error, Result};
use cached::proc_macro::cached;
use oxigraph::model::Term;
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

/// Configuration for entity linking
#[derive(Debug, Clone)]
pub struct EntityLinkerConfig {
    /// Base URL for the entity linking service (for remote strategies)
    pub service_url: String,
    /// Minimum confidence threshold (0.0 - 1.0)
    pub confidence_threshold: f64,
    /// Request timeout in seconds
    pub timeout_secs: u64,
    /// Whether entity linking is enabled
    pub enabled: bool,
    /// Entity linking strategy
    pub strategy: LinkingStrategy,
    /// Path to local RDF knowledge base (for Local strategy)
    pub local_kb_path: Option<PathBuf>,
    /// Use fuzzy matching for candidate retrieval (Levenshtein distance)
    pub use_fuzzy_matching: bool,
    /// Minimum similarity threshold for fuzzy matching (0.0-1.0)
    pub fuzzy_threshold: f64,
    /// Use LLM disambiguation when multiple candidates exist
    pub use_llm_disambiguation: bool,
    /// Minimum number of candidates to trigger LLM disambiguation
    pub min_candidates_for_llm: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub enum LinkingStrategy {
    /// Use local Oxigraph-based linking (Recommended for production)
    Local,
    /// Use DBpedia Spotlight API
    DbpediaSpotlight,
    /// Use Wikidata API
    Wikidata,
    /// Disable entity linking (use normalized names only)
    None,
}

impl Default for EntityLinkerConfig {
    fn default() -> Self {
        Self {
            service_url: "https://api.dbpedia-spotlight.org/en".to_string(),
            confidence_threshold: 0.5,
            timeout_secs: 10,
            enabled: false,
            strategy: LinkingStrategy::None,
            local_kb_path: None,
            use_fuzzy_matching: true,
            fuzzy_threshold: 0.8,
            use_llm_disambiguation: true,
            min_candidates_for_llm: 2,
        }
    }
}

/// Entity linker that resolves entity names to canonical URIs
pub struct EntityLinker {
    config: EntityLinkerConfig,
    /// Local RDF store for Oxigraph-based linking
    store: Option<Arc<Store>>,
    /// GenAI client for LLM dis ambiguation
    llm_client: Option<genai::Client>,
}

impl std::fmt::Debug for EntityLinker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityLinker")
            .field("config", &self.config)
            .field("store", &self.store.as_ref().map(|_| "Store"))
            .field("llm_client", &self.llm_client.as_ref().map(|_| "Client"))
            .finish()
    }
}

/// Result of entity linking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LinkedEntity {
    /// Original surface form (text)
    pub surface_form: String,
    /// Canonical URI (e.g., http://dbpedia.org/resource/Alan_Bean)
    pub uri: String,
    /// Entity types (e.g., ["Person", "Astronaut"])
    pub types: Vec<String>,
    /// Confidence score (0.0 - 1.0)
    pub confidence: f64,
}

#[derive(Debug, Deserialize)]
struct DbpediaSpotlightResponse {
    #[serde(rename = "Resources")]
    resources: Option<Vec<DbpediaResource>>,
}

#[derive(Debug, Deserialize)]
struct DbpediaResource {
    #[serde(rename = "@URI")]
    uri: String,
    #[serde(rename = "@surfaceForm")]
    surface_form: String,
    #[serde(rename = "@types")]
    types: String,
    #[serde(rename = "@similarityScore")]
    confidence: f64,
}

impl EntityLinker {
    /// Create a new entity linker with the given configuration
    pub fn new(config: EntityLinkerConfig) -> Result<Self> {
        let store = if config.strategy == LinkingStrategy::Local {
            // Load local RDF knowledge base from filesystem
            if let Some(kb_path) = &config.local_kb_path {
                let store = Store::open(kb_path)
                    .map_err(|e| Error::Config(format!("Failed to open local KB at {:?}: {}", kb_path, e)))?;
                Some(Arc::new(store))
            } else {
                return Err(Error::Config(
                    "Local strategy requires local_kb_path to be set".to_string()
                ));
            }
        } else {
            None
        };

        // Initialize LLM client if disambiguation is enabled
        let llm_client = if config.use_llm_disambiguation {
            Some(genai::Client::default())
        } else {
            None
        };

        Ok(Self { config, store, llm_client })
    }

    /// Link an entity name to a canonical URI
    ///
    /// Returns None if linking is disabled or no match found above confidence threshold
    pub async fn link_entity(
        &self,
        text: &str,
        entity_name: &str,
        _entity_type: Option<&str>,
    ) -> Result<Option<LinkedEntity>> {
        if !self.config.enabled {
            return Ok(None);
        }

        match self.config.strategy {
            LinkingStrategy::Local => {
                self.link_with_local(entity_name, _entity_type).await
            }
            LinkingStrategy::DbpediaSpotlight => {
                self.link_with_dbpedia(text, entity_name).await
            }
            LinkingStrategy::Wikidata => {
                // Wikidata API implementation would go here
                Ok(None)
            }
            LinkingStrategy::None => Ok(None),
        }
    }

    /// Link entity using DBpedia Spotlight API
    async fn link_with_dbpedia(
        &self,
        text: &str,
        entity_name: &str,
    ) -> Result<Option<LinkedEntity>> {
        // Use cached version to avoid repeated API calls
        link_with_dbpedia_cached(
            self.config.service_url.clone(),
            text.to_string(),
            self.config.confidence_threshold,
        )
        .await
        .map(|entities| {
            // Find the entity that best matches the given name
            entities
                .into_iter()
                .find(|e| e.surface_form.to_lowercase() == entity_name.to_lowercase())
        })
    }

    /// Link entity using local Oxigraph-based knowledge base
    ///
    /// Performs SPARQL query against local RDF store to find matching entities.
    /// Supports both exact matching and fuzzy matching with LLM-based disambiguation.
    async fn link_with_local(
        &self,
        entity_name: &str,
        entity_type: Option<&str>,
    ) -> Result<Option<LinkedEntity>> {
        let store = self.store.as_ref().ok_or_else(|| {
            Error::Config("Local store not initialized".to_string())
        })?;

        // Step 1: Retrieve candidates using fuzzy or exact matching
        let mut candidates = if self.config.use_fuzzy_matching {
            self.fuzzy_search_candidates(store, entity_name)?
        } else {
            self.exact_search_candidates(store, entity_name)?
        };

        // Step 2: Filter by confidence threshold
        candidates.retain(|c| c.confidence >= self.config.confidence_threshold);

        if candidates.is_empty() {
            return Ok(None);
        }

        // Step 3: If multiple candidates exist, use LLM disambiguation
        if candidates.len() >= self.config.min_candidates_for_llm
            && self.config.use_llm_disambiguation {
            return self.disambiguate_with_llm(entity_name, entity_type, &candidates).await;
        }

        // Step 4: Return best match (highest confidence)
        candidates.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());
        Ok(candidates.into_iter().next())
    }

    /// Exact search for entity labels (original behavior)
    fn exact_search_candidates(
        &self,
        store: &Store,
        entity_name: &str,
    ) -> Result<Vec<LinkedEntity>> {
        let query = format!(
            r#"
            PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
            PREFIX schema: <http://schema.org/>
            PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>

            SELECT ?entity ?label ?type WHERE {{
                {{
                    ?entity rdfs:label ?label .
                    FILTER(LCASE(STR(?label)) = LCASE("{}"))
                }} UNION {{
                    ?entity schema:name ?label .
                    FILTER(LCASE(STR(?label)) = LCASE("{}"))
                }}
                OPTIONAL {{ ?entity rdf:type ?type }}
            }}
            LIMIT 10
            "#,
            entity_name.replace('"', "\\\""),
            entity_name.replace('"', "\\\"")
        );

        self.execute_candidate_query(store, &query, entity_name, true)
    }

    /// Fuzzy search using Levenshtein/Jaro-Winkler distance
    fn fuzzy_search_candidates(
        &self,
        store: &Store,
        entity_name: &str,
    ) -> Result<Vec<LinkedEntity>> {
        // Query for similar labels (broader search)
        let query = format!(
            r#"
            PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
            PREFIX schema: <http://schema.org/>
            PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>

            SELECT ?entity ?label ?type WHERE {{
                {{
                    ?entity rdfs:label ?label .
                    FILTER(CONTAINS(LCASE(STR(?label)), LCASE("{}")))
                }} UNION {{
                    ?entity schema:name ?label .
                    FILTER(CONTAINS(LCASE(STR(?label)), LCASE("{}")))
                }}
                OPTIONAL {{ ?entity rdf:type ?type }}
            }}
            LIMIT 50
            "#,
            entity_name.replace('"', "\\\""),
            entity_name.replace('"', "\\\"")
        );

        self.execute_candidate_query(store, &query, entity_name, false)
    }

    /// Execute SPARQL query and calculate confidence scores
    ///
    /// TODO: Update to use SparqlEvaluator interface when oxigraph 0.5 API is fully documented
    #[allow(deprecated)]
    fn execute_candidate_query(
        &self,
        store: &Store,
        query: &str,
        entity_name: &str,
        exact_match: bool,
    ) -> Result<Vec<LinkedEntity>> {
        let results = store
            .query(query)
            .map_err(|e| Error::Extraction(format!("SPARQL query failed: {}", e)))?;

        let mut candidates = Vec::new();

        if let QueryResults::Solutions(solutions) = results {
            for solution in solutions {
                let solution = solution
                    .map_err(|e| Error::Extraction(format!("Query solution error: {}", e)))?;

                if let Some(Term::NamedNode(entity_node)) = solution.get("entity") {
                    let uri = entity_node.as_str().to_string();

                    let label = solution
                        .get("label")
                        .and_then(|t| {
                            if let Term::Literal(lit) = t {
                                Some(lit.value().to_string())
                            } else {
                                None
                            }
                        })
                        .unwrap_or_else(|| entity_name.to_string());

                    let type_str = solution
                        .get("type")
                        .and_then(|t| {
                            if let Term::NamedNode(node) = t {
                                Some(node.as_str().to_string())
                            } else {
                                None
                            }
                        });

                    let types = if let Some(t) = type_str {
                        vec![t]
                    } else {
                        Vec::new()
                    };

                    // Calculate confidence using Jaro-Winkler similarity
                    let confidence = if exact_match {
                        if label.to_lowercase() == entity_name.to_lowercase() {
                            0.95
                        } else {
                            0.7
                        }
                    } else {
                        // Use Jaro-Winkler for fuzzy matching
                        let similarity = strsim::jaro_winkler(
                            &label.to_lowercase(),
                            &entity_name.to_lowercase(),
                        );

                        // Only include candidates above fuzzy threshold
                        if similarity < self.config.fuzzy_threshold {
                            continue;
                        }

                        similarity
                    };

                    candidates.push(LinkedEntity {
                        surface_form: label,
                        uri,
                        types,
                        confidence,
                    });
                }
            }
        }

        // Sort by confidence
        candidates.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

        Ok(candidates)
    }

    /// Use LLM to disambiguate between multiple entity candidates
    ///
    /// Sends candidates to LLM with context and asks it to select the best match.
    /// Example: "Apple" in "I ate an Apple" → fruit, not tech company
    async fn disambiguate_with_llm(
        &self,
        entity_name: &str,
        entity_type: Option<&str>,
        candidates: &[LinkedEntity],
    ) -> Result<Option<LinkedEntity>> {
        let llm_client = self.llm_client.as_ref().ok_or_else(|| {
            Error::Config("LLM client not initialized for disambiguation".to_string())
        })?;

        // Format candidates for LLM
        let candidate_list: Vec<String> = candidates
            .iter()
            .enumerate()
            .map(|(i, c)| {
                format!(
                    "{}. {} (URI: {}, Types: [{}], Confidence: {:.2})",
                    i + 1,
                    c.surface_form,
                    c.uri,
                    c.types.join(", "),
                    c.confidence
                )
            })
            .collect();

        let prompt = format!(
            r#"Given the entity name "{}" and the following candidates from a knowledge base, select the most appropriate match.

Context: {}

Candidates:
{}

Respond with ONLY the number (1-{}) of the best matching candidate. Consider:
- Semantic context and entity type
- URI authority (Wikidata vs DBpedia)
- Entity types and their relevance
- Confidence scores

Your response (just the number):"#,
            entity_name,
            entity_type.unwrap_or("No specific type provided"),
            candidate_list.join("\n"),
            candidates.len()
        );

        // Call LLM
        let user_msg = genai::chat::ChatMessage::user(prompt);
        let chat_req = genai::chat::ChatRequest::new(vec![user_msg]);

        let response = llm_client
            .exec_chat(&self.config.service_url, chat_req, None)
            .await
            .map_err(|e| Error::Network(format!("LLM disambiguation failed: {}", e)))?;

        // Parse LLM response using genai 0.5 API
        let response_text = response.first_text().unwrap_or("");
        let selected_idx: usize = response_text
            .trim()
            .parse()
            .map_err(|_| Error::Extraction(format!("Invalid LLM response: {}", response_text)))?;

        if selected_idx > 0 && selected_idx <= candidates.len() {
            Ok(Some(candidates[selected_idx - 1].clone()))
        } else {
            Err(Error::Extraction(format!(
                "LLM selected invalid candidate index: {}",
                selected_idx
            )))
        }
    }

    /// Batch link multiple entities from the same text
    pub async fn link_entities(
        &self,
        text: &str,
        entity_names: &[String],
    ) -> Result<Vec<Option<LinkedEntity>>> {
        let mut results = Vec::new();

        for name in entity_names {
            let linked = self.link_entity(text, name, None).await?;
            results.push(linked);
        }

        Ok(results)
    }
}

/// Cached DBpedia Spotlight API call
///
/// Caches results for 1 hour to reduce API load
#[cached(
    time = 3600,
    result = true,
    key = "String",
    convert = r#"{ format!("{}-{}", service_url, text) }"#
)]
async fn link_with_dbpedia_cached(
    service_url: String,
    text: String,
    confidence_threshold: f64,
) -> Result<Vec<LinkedEntity>> {
    let client = Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .map_err(|e| Error::Network(e.to_string()))?;

    let url = format!("{}/annotate", service_url);

    let response = client
        .post(&url)
        .header("Accept", "application/json")
        .form(&[
            ("text", text.as_str()),
            ("confidence", &confidence_threshold.to_string()),
        ])
        .send()
        .await
        .map_err(|e| Error::Network(format!("DBpedia Spotlight request failed: {}", e)))?;

    if !response.status().is_success() {
        return Ok(Vec::new());
    }

    let spotlight_response: DbpediaSpotlightResponse = response
        .json()
        .await
        .map_err(|e| Error::Network(format!("Failed to parse DBpedia response: {}", e)))?;

    let entities = spotlight_response
        .resources
        .unwrap_or_default()
        .into_iter()
        .map(|resource| LinkedEntity {
            surface_form: resource.surface_form,
            uri: resource.uri,
            types: resource
                .types
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            confidence: resource.confidence,
        })
        .collect();

    Ok(entities)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = EntityLinkerConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.strategy, LinkingStrategy::None);
        assert!((config.confidence_threshold - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_config_builder() {
        let config = EntityLinkerConfig {
            enabled: true,
            strategy: LinkingStrategy::DbpediaSpotlight,
            confidence_threshold: 0.7,
            ..Default::default()
        };

        assert!(config.enabled);
        assert_eq!(config.strategy, LinkingStrategy::DbpediaSpotlight);
        assert!((config.confidence_threshold - 0.7).abs() < f64::EPSILON);
    }

    #[tokio::test]
    async fn test_linker_creation() {
        let config = EntityLinkerConfig::default();
        let linker = EntityLinker::new(config);
        assert!(linker.is_ok());
    }

    #[tokio::test]
    async fn test_disabled_linker() {
        let config = EntityLinkerConfig::default(); // disabled by default
        let linker = EntityLinker::new(config).unwrap();

        let result = linker
            .link_entity("Alan Bean was an astronaut", "Alan Bean", Some("Person"))
            .await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    // Integration test with real DBpedia API (ignored by default)
    #[tokio::test]
    #[ignore = "requires external DBpedia API"]
    async fn test_dbpedia_linking() {
        let config = EntityLinkerConfig {
            enabled: true,
            strategy: LinkingStrategy::DbpediaSpotlight,
            confidence_threshold: 0.5,
            ..Default::default()
        };

        let entity_linker = EntityLinker::new(config).unwrap();

        let result = entity_linker
            .link_entity("Alan Bean was an astronaut", "Alan Bean", Some("Person"))
            .await;

        // Print error if it fails
        if let Err(ref e) = result {
            eprintln!("DBpedia linking error: {}", e);
        }

        assert!(result.is_ok(), "DBpedia API call failed");
        let link_result = result.unwrap();
        assert!(link_result.is_some(), "No entity found");

        let entity = link_result.unwrap();
        assert!(entity.uri.contains("dbpedia.org"));
        assert!(entity.confidence > 0.5);
    }

    #[test]
    fn test_local_strategy_requires_path() {
        let config = EntityLinkerConfig {
            enabled: true,
            strategy: LinkingStrategy::Local,
            local_kb_path: None,
            ..Default::default()
        };

        let linker = EntityLinker::new(config);
        assert!(linker.is_err());
        assert!(linker.unwrap_err().to_string().contains("local_kb_path"));
    }
}
