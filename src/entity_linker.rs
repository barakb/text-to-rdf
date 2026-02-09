//! Entity Linking Module - Stage 2 of the RDF extraction pipeline
//!
//! Links extracted entity names to canonical URIs from knowledge bases.
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
        }
    }
}

/// Entity linker that resolves entity names to canonical URIs
pub struct EntityLinker {
    config: EntityLinkerConfig,
    /// Local RDF store for Oxigraph-based linking
    store: Option<Arc<Store>>,
}

impl std::fmt::Debug for EntityLinker {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EntityLinker")
            .field("config", &self.config)
            .field("store", &self.store.as_ref().map(|_| "Store"))
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

        Ok(Self { config, store })
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
    /// Performs SPARQL query against local RDF store to find matching entities
    async fn link_with_local(
        &self,
        entity_name: &str,
        _entity_type: Option<&str>,
    ) -> Result<Option<LinkedEntity>> {
        let store = self.store.as_ref().ok_or_else(|| {
            Error::Config("Local store not initialized".to_string())
        })?;

        // Query for entities with matching labels
        // Supports both Wikidata (wd:Q*) and DBpedia (dbr:*) URIs
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

        // Execute SPARQL query
        let results = store
            .query(&query)
            .map_err(|e| Error::Extraction(format!("SPARQL query failed: {}", e)))?;

        // Process query results
        if let QueryResults::Solutions(solutions) = results {
            let mut candidates = Vec::new();

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

                    // Extract types from query results
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

                    // Calculate confidence based on exact match
                    let confidence = if label.to_lowercase() == entity_name.to_lowercase() {
                        0.95
                    } else {
                        0.7
                    };

                    candidates.push(LinkedEntity {
                        surface_form: label,
                        uri,
                        types,
                        confidence,
                    });
                }
            }

            // Filter by confidence threshold and return best match
            candidates.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap());

            return Ok(candidates
                .into_iter()
                .find(|c| c.confidence >= self.config.confidence_threshold));
        }

        Ok(None)
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
        assert_eq!(config.confidence_threshold, 0.5);
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
        assert_eq!(config.confidence_threshold, 0.7);
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
    #[ignore]
    async fn test_dbpedia_linking() {
        let config = EntityLinkerConfig {
            enabled: true,
            strategy: LinkingStrategy::DbpediaSpotlight,
            confidence_threshold: 0.5,
            ..Default::default()
        };

        let linker = EntityLinker::new(config).unwrap();

        let result = linker
            .link_entity("Alan Bean was an astronaut", "Alan Bean", Some("Person"))
            .await;

        assert!(result.is_ok());
        let linked = result.unwrap();
        assert!(linked.is_some());

        let entity = linked.unwrap();
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
