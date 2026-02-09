//! GLiNER-based Entity Extraction - Stage 1 of the RDF extraction pipeline
//!
//! Uses GLiNER (Generalist and Lightweight Named Entity Recognition) for zero-shot
//! entity extraction. This provides:
//! - High recall (doesn't miss entities like LLMs sometimes do)
//! - Provenance (exact character offsets for each entity)
//! - No hallucinations (only returns what's in the text)
//! - 4x faster than Python GLiNER
//! - No API costs (runs locally)
//!
//! GLiNER is particularly good for the Discovery phase, finding all entities
//! with their exact locations in the text. Relations can then be extracted by
//! an LLM or rule-based system.

#[cfg(feature = "gliner")]
use crate::error::{Error, Result};
#[cfg(feature = "gliner")]
use crate::types::{RdfDocument, EntityType};
#[cfg(feature = "gliner")]
use crate::RdfExtractor;
#[cfg(feature = "gliner")]
use async_trait::async_trait;
#[cfg(feature = "gliner")]
use gliner::model::{GLiNER, input::text::TextInput, params::Parameters};
#[cfg(feature = "gliner")]
use gliner::model::pipeline::span::SpanMode;
#[cfg(feature = "gliner")]
use orp::params::RuntimeParameters;
#[cfg(feature = "gliner")]
use serde_json::json;
#[cfg(feature = "gliner")]
use std::collections::HashMap;
#[cfg(feature = "gliner")]
use std::path::PathBuf;

#[cfg(feature = "gliner")]
/// Extracted entity with provenance: (text, entity_type, confidence, start_offset, end_offset)
type ExtractedEntity = (String, String, f32, usize, usize);

#[cfg(feature = "gliner")]
/// Configuration for GLiNER-based extraction
#[derive(Debug, Clone)]
pub struct GlinerConfig {
    /// Path to GLiNER ONNX model directory
    pub model_path: PathBuf,

    /// Entity types to extract (Schema.org types or custom)
    /// Examples: ["Person", "Organization", "Place", "Event", "Product"]
    pub entity_types: Vec<String>,

    /// Confidence threshold (0.0-1.0)
    pub confidence_threshold: f32,

    /// Flatten overlapping entities (recommended: true)
    pub flat_ner: bool,

    /// Number of threads for inference (0 = auto)
    pub num_threads: usize,
}

#[cfg(feature = "gliner")]
impl Default for GlinerConfig {
    fn default() -> Self {
        Self {
            model_path: PathBuf::from("models/gliner_medium-v2.1"),
            entity_types: vec![
                "Person".to_string(),
                "Organization".to_string(),
                "Place".to_string(),
                "Event".to_string(),
                "Product".to_string(),
                "Date".to_string(),
            ],
            confidence_threshold: 0.5,
            flat_ner: true,
            num_threads: 0, // Auto-detect
        }
    }
}

#[cfg(feature = "gliner")]
impl GlinerConfig {
    /// Load configuration from environment variables
    ///
    /// Supported environment variables:
    /// - `GLINER_MODEL_PATH`: Path to ONNX model directory
    /// - `GLINER_ENTITY_TYPES`: Comma-separated list of entity types
    /// - `GLINER_CONFIDENCE`: Confidence threshold (0.0-1.0)
    /// - `GLINER_THREADS`: Number of threads (0 = auto)
    pub fn from_env() -> Result<Self> {
        dotenvy::dotenv().ok();

        let model_path = std::env::var("GLINER_MODEL_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("models/gliner_medium-v2.1"));

        let entity_types = std::env::var("GLINER_ENTITY_TYPES")
            .map(|s| s.split(',').map(|t| t.trim().to_string()).collect())
            .unwrap_or_else(|_| Self::default().entity_types);

        let confidence_threshold = std::env::var("GLINER_CONFIDENCE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok())
            .unwrap_or(0.5);

        let num_threads = std::env::var("GLINER_THREADS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(0);

        Ok(Self {
            model_path,
            entity_types,
            confidence_threshold,
            flat_ner: true,
            num_threads,
        })
    }
}

#[cfg(feature = "gliner")]
/// GLiNER-based extractor for zero-shot Named Entity Recognition
pub struct GlinerExtractor {
    model: GLiNER<SpanMode>,
    config: GlinerConfig,
}

#[cfg(feature = "gliner")]
impl GlinerExtractor {
    /// Create a new GLiNER extractor
    ///
    /// # Arguments
    ///
    /// * `config` - GLiNER configuration with model path and entity types
    ///
    /// # Errors
    ///
    /// Returns an error if the model cannot be loaded
    pub fn new(config: GlinerConfig) -> Result<Self> {
        // Validate model path
        if !config.model_path.exists() {
            return Err(Error::Config(format!(
                "GLiNER model not found at {:?}. Download from HuggingFace:\n\
                 huggingface-cli download onnx-community/gliner_medium-v2.1",
                config.model_path
            )));
        }

        // Construct paths to tokenizer and model files
        let tokenizer_path = config.model_path.join("tokenizer.json");
        let model_onnx_path = config.model_path.join("onnx/model.onnx");

        if !tokenizer_path.exists() {
            return Err(Error::Config(format!(
                "Tokenizer not found at {:?}",
                tokenizer_path
            )));
        }

        if !model_onnx_path.exists() {
            return Err(Error::Config(format!(
                "ONNX model not found at {:?}",
                model_onnx_path
            )));
        }

        // Configure runtime parameters
        let mut runtime_params = RuntimeParameters::default();
        if config.num_threads > 0 {
            runtime_params = runtime_params.with_threads(config.num_threads);
        }

        // Load model
        let model = GLiNER::<SpanMode>::new(
            Parameters::default(),
            runtime_params,
            tokenizer_path.to_str().ok_or_else(|| {
                Error::Config("Invalid tokenizer path".to_string())
            })?,
            model_onnx_path.to_str().ok_or_else(|| {
                Error::Config("Invalid model path".to_string())
            })?,
        )
        .map_err(|e| Error::Config(format!("Failed to load GLiNER model: {}", e)))?;

        Ok(Self { model, config })
    }

    /// Extract entities with provenance (character offsets)
    ///
    /// Returns entities with exact start/end positions in the original text
    fn extract_entities_with_provenance(
        &self,
        text: &str,
    ) -> Result<Vec<ExtractedEntity>> {
        // Create input with single text and configured entity types
        let entity_type_refs: Vec<&str> = self.config.entity_types.iter().map(|s| s.as_str()).collect();

        let input = TextInput::from_str(&[text], &entity_type_refs)
            .map_err(|e| Error::Extraction(format!("Failed to create TextInput: {}", e)))?;

        // Run inference
        let output = self.model.inference(input)
            .map_err(|e| Error::Extraction(format!("GLiNER inference failed: {}", e)))?;

        // Extract spans from first result (we only passed one text)
        let mut entities = Vec::new();
        if let Some(spans) = output.spans.first() {
            for span in spans {
                let confidence = span.probability();

                // Filter by confidence threshold
                if confidence >= self.config.confidence_threshold {
                    let (start, end) = span.offsets();
                    entities.push((
                        span.text().to_string(),
                        span.class().to_string(),
                        confidence,
                        start,
                        end,
                    ));
                }
            }
        }

        Ok(entities)
    }

    /// Map GLiNER entity type to Schema.org type
    fn map_to_schema_type(&self, gliner_type: &str) -> EntityType {
        match gliner_type.to_lowercase().as_str() {
            "person" => EntityType::Person,
            "organization" | "organisation" | "company" => EntityType::Organization,
            "place" | "location" | "city" | "country" => EntityType::Place,
            "event" => EntityType::Event,
            _ => EntityType::Custom(gliner_type.to_string()),
        }
    }
}

#[cfg(feature = "gliner")]
#[async_trait]
impl RdfExtractor for GlinerExtractor {
    /// Extract RDF entities from text using GLiNER
    ///
    /// # Arguments
    ///
    /// * `text` - Input text to extract entities from
    ///
    /// # Returns
    ///
    /// An `RdfDocument` containing extracted entities with provenance metadata
    ///
    /// # Errors
    ///
    /// Returns an error if GLiNER inference fails
    async fn extract(&self, text: &str) -> Result<RdfDocument> {
        // Extract entities with GLiNER
        let gliner_entities = self.extract_entities_with_provenance(text)?;

        // If only one entity, create single entity document
        if gliner_entities.len() == 1 {
            let (entity_text, entity_type, confidence, start, end) = &gliner_entities[0];
            let schema_type = self.map_to_schema_type(entity_type);

            let mut properties = HashMap::new();
            properties.insert("_text".to_string(), json!(entity_text));
            properties.insert("_startOffset".to_string(), json!(start));
            properties.insert("_endOffset".to_string(), json!(end));
            properties.insert("_confidence".to_string(), json!(confidence));
            properties.insert("_glinerType".to_string(), json!(entity_type));

            let data = json!({
                "@context": "https://schema.org/",
                "@type": serialize_entity_type(&schema_type),
                "@id": format!("entity_{}", start),
                "name": entity_text,
                "_metadata": {
                    "text": entity_text,
                    "startOffset": start,
                    "endOffset": end,
                    "confidence": confidence,
                    "glinerType": entity_type,
                    "extractor": "GLiNER"
                }
            });

            return RdfDocument::from_value(data);
        }

        // Multiple entities - create graph
        let mut entities_json = Vec::new();
        for (entity_text, entity_type, confidence, start, end) in gliner_entities {
            let schema_type = self.map_to_schema_type(&entity_type);

            entities_json.push(json!({
                "@id": format!("entity_{}", start),
                "@type": serialize_entity_type(&schema_type),
                "name": entity_text,
                "_metadata": {
                    "text": entity_text,
                    "startOffset": start,
                    "endOffset": end,
                    "confidence": confidence,
                    "glinerType": entity_type,
                }
            }));
        }

        let data = json!({
            "@context": "https://schema.org/",
            "@graph": entities_json,
            "_extractionMetadata": {
                "extractor": "GLiNER",
                "model": self.config.model_path.display().to_string(),
                "extractedEntities": entities_json.len(),
                "sourceTextLength": text.len()
            }
        });

        RdfDocument::from_value(data)
    }
}

#[cfg(feature = "gliner")]
/// Helper function to serialize EntityType to string for JSON
fn serialize_entity_type(entity_type: &EntityType) -> String {
    match entity_type {
        EntityType::Person => "Person".to_string(),
        EntityType::Organization => "Organization".to_string(),
        EntityType::EducationalOrganization => "EducationalOrganization".to_string(),
        EntityType::Place => "Place".to_string(),
        EntityType::Event => "Event".to_string(),
        EntityType::Country => "Country".to_string(),
        EntityType::Award => "Award".to_string(),
        EntityType::Custom(s) => s.clone(),
    }
}

// Stub implementations when feature is disabled
#[cfg(not(feature = "gliner"))]
pub struct GlinerConfig;

#[cfg(not(feature = "gliner"))]
pub struct GlinerExtractor;

#[cfg(not(feature = "gliner"))]
impl GlinerExtractor {
    pub fn new(_config: GlinerConfig) -> Result<Self, crate::error::Error> {
        Err(crate::error::Error::Config(
            "GLiNER feature not enabled. Rebuild with --features gliner".to_string()
        ))
    }
}

#[cfg(all(test, feature = "gliner"))]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = GlinerConfig::default();
        assert_eq!(config.confidence_threshold, 0.5);
        assert!(config.flat_ner);
        assert!(config.entity_types.contains(&"Person".to_string()));
    }

    #[test]
    fn test_schema_mapping() {
        // Test the mapping function without needing a model
        assert_eq!(
            serialize_entity_type(&EntityType::Person),
            "Person"
        );
        assert_eq!(
            serialize_entity_type(&EntityType::Organization),
            "Organization"
        );
    }
}
