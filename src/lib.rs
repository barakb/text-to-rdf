//! # Text to RDF Library
//!
//! A high-performance Rust library for extracting structured RDF data (entities and relations)
//! from unstructured text using LLMs via the `genai` crate.
//!
//! ## Features
//!
//! - Schema-First Extraction: Outputs JSON-LD mapped to Schema.org and standard RDF ontologies
//! - Multi-Provider AI Support: Works with Gemini, Claude, GPT via `genai`
//! - Trait-Based Design: Extensible architecture for custom extractors
//! - Environment Variable Support: Load configuration from .env files
//!
//! ## Example
//!
//! ```rust,no_run
//! use text_to_rdf::{RdfExtractor, GenAiExtractor, ExtractionConfig};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     // Load from .env file
//!     let config = ExtractionConfig::from_env()?;
//!
//!     let extractor = GenAiExtractor::new(config)?;
//!     let text = "Albert Einstein was born in Ulm, Germany on March 14, 1879.";
//!
//!     let result = extractor.extract(text).await?;
//!     println!("{}", serde_json::to_string_pretty(&result)?);
//!
//!     Ok(())
//! }
//! ```

use async_trait::async_trait;
use std::env;

pub mod error;
pub mod extractor;
pub mod types;
pub mod normalize;
pub mod entity_linker;
pub mod validation;
pub mod gliner_extractor;

pub use error::{Error, Result};
pub use extractor::GenAiExtractor;
pub use types::{RdfDocument, RdfEntity, EntityType};
pub use entity_linker::{EntityLinker, EntityLinkerConfig, LinkedEntity, LinkingStrategy};
pub use validation::{RdfValidator, ValidationResult, ValidationRule, Violation, Severity};

#[cfg(feature = "gliner")]
pub use gliner_extractor::{GlinerExtractor, GlinerConfig};

/// Initialize the library by loading .env file
///
/// This should be called at the start of your application to load environment variables
/// from a .env file in the current directory or parent directories.
///
/// # Errors
///
/// Returns an error if the .env file exists but cannot be read or parsed
pub fn init() -> Result<()> {
    dotenvy::dotenv().ok(); // Ignore if .env doesn't exist
    Ok(())
}

/// Configuration for RDF extraction
#[derive(Debug, Clone)]
pub struct ExtractionConfig {
    /// The AI model to use (e.g., "claude-3-5-sonnet", "gpt-4o", "gemini-1.5-pro")
    pub model: String,

    /// Temperature for AI generation (0.0 - 1.0)
    pub temperature: Option<f32>,

    /// Maximum tokens in the response
    pub max_tokens: Option<u32>,

    /// Custom system prompt override
    pub system_prompt: Option<String>,

    /// Target ontologies (default: schema.org)
    pub ontologies: Vec<String>,

    /// Entity linker configuration
    pub entity_linker: EntityLinkerConfig,
}

impl Default for ExtractionConfig {
    fn default() -> Self {
        Self {
            model: "claude-3-5-sonnet".to_string(),
            temperature: Some(0.3),
            max_tokens: Some(4096),
            system_prompt: None,
            ontologies: vec!["https://schema.org/".to_string()],
            entity_linker: EntityLinkerConfig::default(),
        }
    }
}

impl ExtractionConfig {
    /// Create a new configuration with default values
    pub fn new() -> Self {
        Self::default()
    }

    /// Load configuration from environment variables
    ///
    /// Automatically loads .env file if present. Supports these variables:
    /// - `GENAI_API_KEY`: API key for the AI service (required)
    /// - `RDF_EXTRACTION_MODEL`: Model name for entity/relation extraction (default: "claude-3-5-sonnet")
    /// - `GENAI_TEMPERATURE`: Temperature 0.0-1.0 (default: 0.3)
    /// - `GENAI_MAX_TOKENS`: Max tokens (default: 4096)
    /// - `GENAI_SYSTEM_PROMPT`: Custom system prompt
    /// - `RDF_ONTOLOGIES`: Comma-separated ontology URLs
    /// - `ENTITY_LINKING_ENABLED`: Enable entity linking (default: false)
    /// - `ENTITY_LINKING_STRATEGY`: Strategy: "local", "dbpedia", "wikidata", or "none" (default: "none")
    /// - `ENTITY_LINKING_KB_PATH`: Path to local RDF knowledge base (required for "local" strategy)
    /// - `ENTITY_LINKING_SERVICE_URL`: Service URL for remote strategies (default: DBpedia Spotlight)
    /// - `ENTITY_LINKING_CONFIDENCE`: Confidence threshold 0.0-1.0 (default: 0.5)
    ///
    /// # Errors
    ///
    /// Returns an error if required environment variables are missing or invalid
    ///
    /// # Example
    ///
    /// ```rust,no_run
    /// use text_to_rdf::ExtractionConfig;
    ///
    /// let config = ExtractionConfig::from_env().unwrap();
    /// ```
    pub fn from_env() -> Result<Self> {
        // Load .env file
        dotenvy::dotenv().ok();

        // Check for required API key
        if env::var("GENAI_API_KEY").is_err() {
            return Err(Error::Config(
                "GENAI_API_KEY environment variable is required".to_string(),
            ));
        }

        let model = env::var("RDF_EXTRACTION_MODEL")
            .unwrap_or_else(|_| "claude-3-5-sonnet".to_string());

        let temperature = env::var("GENAI_TEMPERATURE")
            .ok()
            .and_then(|v| v.parse::<f32>().ok());

        let max_tokens = env::var("GENAI_MAX_TOKENS")
            .ok()
            .and_then(|v| v.parse::<u32>().ok());

        let system_prompt = env::var("GENAI_SYSTEM_PROMPT").ok();

        let ontologies = env::var("RDF_ONTOLOGIES")
            .map(|v| v.split(',').map(|s| s.trim().to_string()).collect())
            .unwrap_or_else(|_| vec!["https://schema.org/".to_string()]);

        // Entity linker configuration
        let entity_linker_enabled = env::var("ENTITY_LINKING_ENABLED")
            .ok()
            .and_then(|v| v.parse::<bool>().ok())
            .unwrap_or(false);

        let entity_linker_strategy = match env::var("ENTITY_LINKING_STRATEGY")
            .unwrap_or_else(|_| "none".to_string())
            .to_lowercase()
            .as_str()
        {
            "local" => LinkingStrategy::Local,
            "dbpedia" | "dbpedia_spotlight" => LinkingStrategy::DbpediaSpotlight,
            "wikidata" => LinkingStrategy::Wikidata,
            _ => LinkingStrategy::None,
        };

        let entity_linker_service_url = env::var("ENTITY_LINKING_SERVICE_URL")
            .unwrap_or_else(|_| "https://api.dbpedia-spotlight.org/en".to_string());

        let entity_linker_confidence = env::var("ENTITY_LINKING_CONFIDENCE")
            .ok()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.5);

        let entity_linker_kb_path = env::var("ENTITY_LINKING_KB_PATH")
            .ok()
            .map(std::path::PathBuf::from);

        let entity_linker = EntityLinkerConfig {
            enabled: entity_linker_enabled,
            strategy: entity_linker_strategy,
            service_url: entity_linker_service_url,
            confidence_threshold: entity_linker_confidence,
            local_kb_path: entity_linker_kb_path,
            ..EntityLinkerConfig::default()
        };

        Ok(Self {
            model,
            temperature,
            max_tokens,
            system_prompt,
            ontologies,
            entity_linker,
        })
    }

    /// Set the AI model to use
    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model = model.into();
        self
    }

    /// Set the temperature
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = Some(temperature);
        self
    }

    /// Set the maximum tokens
    pub fn with_max_tokens(mut self, max_tokens: u32) -> Self {
        self.max_tokens = Some(max_tokens);
        self
    }

    /// Add an ontology namespace
    pub fn with_ontology(mut self, ontology: impl Into<String>) -> Self {
        self.ontologies.push(ontology.into());
        self
    }

    /// Set custom system prompt
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }
}

/// Main trait for RDF extraction from text
///
/// Implementors of this trait can extract structured RDF entities and relations
/// from unstructured text.
#[async_trait]
pub trait RdfExtractor: Send + Sync {
    /// Extract RDF entities and relations from text
    ///
    /// # Arguments
    ///
    /// * `text` - The input text to process
    ///
    /// # Returns
    ///
    /// An `RdfDocument` containing the extracted entities in JSON-LD format
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The AI service is unavailable
    /// - The response cannot be parsed as valid JSON-LD
    /// - Network issues occur
    async fn extract(&self, text: &str) -> Result<RdfDocument>;

    /// Extract and validate RDF, returning only valid Schema.org entities
    ///
    /// # Arguments
    ///
    /// * `text` - The input text to process
    ///
    /// # Errors
    ///
    /// Returns an error if extraction fails or validation fails
    async fn extract_and_validate(&self, text: &str) -> Result<RdfDocument> {
        let doc = self.extract(text).await?;
        // Basic validation - ensure @context and @type exist
        doc.validate()?;
        Ok(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_builder() {
        let config = ExtractionConfig::new()
            .with_model("gpt-4o")
            .with_temperature(0.5)
            .with_max_tokens(2000);

        assert_eq!(config.model, "gpt-4o");
        assert_eq!(config.temperature, Some(0.5));
        assert_eq!(config.max_tokens, Some(2000));
    }

    #[test]
    fn test_default_config() {
        let config = ExtractionConfig::default();
        assert_eq!(config.model, "claude-3-5-sonnet");
        assert!(config.ontologies.contains(&"https://schema.org/".to_string()));
    }

    #[test]
    fn test_config_with_system_prompt() {
        let config = ExtractionConfig::new()
            .with_system_prompt("Custom prompt");

        assert_eq!(config.system_prompt, Some("Custom prompt".to_string()));
    }

    #[test]
    fn test_init() {
        // Should not fail even if .env doesn't exist
        assert!(init().is_ok());
    }

    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}


