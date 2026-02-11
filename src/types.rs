//! Core RDF types and structures

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::{Error, Result};

/// Hardcoded JSON-LD @context to ensure correct URIs
/// This prevents LLM hallucinations of incorrect Schema.org URIs
const HARDCODED_CONTEXT: &str = include_str!("../context.jsonld");

/// Entity types from Schema.org ontology
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum EntityType {
    #[serde(rename = "Person")]
    Person,
    #[serde(rename = "Organization")]
    Organization,
    #[serde(rename = "EducationalOrganization")]
    EducationalOrganization,
    #[serde(rename = "Place")]
    Place,
    #[serde(rename = "Event")]
    Event,
    #[serde(rename = "Country")]
    Country,
    #[serde(rename = "Award")]
    Award,
    #[serde(untagged)]
    Custom(String),
}

/// An RDF entity representing a Schema.org object
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdfEntity {
    #[serde(rename = "@type")]
    pub entity_type: EntityType,

    #[serde(rename = "@id", skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,

    #[serde(flatten)]
    pub properties: HashMap<String, Value>,
}

impl RdfEntity {
    /// Create a new RDF entity
    #[must_use]
    pub fn new(entity_type: EntityType) -> Self {
        Self {
            entity_type,
            id: None,
            name: None,
            properties: HashMap::new(),
        }
    }

    /// Set the entity ID
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Set the entity name
    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Add a property
    #[must_use]
    pub fn with_property(mut self, key: impl Into<String>, value: Value) -> Self {
        self.properties.insert(key.into(), value);
        self
    }

    /// Get a property value
    #[must_use]
    pub fn get_property(&self, key: &str) -> Option<&Value> {
        self.properties.get(key)
    }
}

/// Provenance metadata for tracking extraction source and confidence
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// Character offset range in source document (start, end)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text_span: Option<(usize, usize)>,

    /// Confidence score (0.0-1.0) for this extraction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,

    /// Source chunk ID (for multi-chunk documents)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub chunk_id: Option<usize>,

    /// Extraction method used
    #[serde(skip_serializing_if = "Option::is_none")]
    pub method: Option<String>, // "llm", "gliner", "rule-based"

    /// Source text that supports this extraction
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_text: Option<String>,
}

impl Provenance {
    /// Create a new provenance record
    #[must_use]
    pub fn new() -> Self {
        Self {
            text_span: None,
            confidence: None,
            chunk_id: None,
            method: None,
            source_text: None,
        }
    }

    /// Set text span
    #[must_use]
    pub fn with_text_span(mut self, start: usize, end: usize) -> Self {
        self.text_span = Some((start, end));
        self
    }

    /// Set confidence score
    #[must_use]
    pub fn with_confidence(mut self, confidence: f64) -> Self {
        self.confidence = Some(confidence);
        self
    }

    /// Set chunk ID
    #[must_use]
    pub fn with_chunk_id(mut self, chunk_id: usize) -> Self {
        self.chunk_id = Some(chunk_id);
        self
    }

    /// Set extraction method
    #[must_use]
    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = Some(method.into());
        self
    }

    /// Set source text
    #[must_use]
    pub fn with_source_text(mut self, text: impl Into<String>) -> Self {
        self.source_text = Some(text.into());
        self
    }
}

impl Default for Provenance {
    fn default() -> Self {
        Self::new()
    }
}

/// An RDF document containing entities in JSON-LD format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdfDocument {
    #[serde(rename = "@context")]
    pub context: Value,

    #[serde(flatten)]
    pub data: Value,

    /// Optional provenance metadata (not serialized to JSON-LD by default)
    #[serde(skip)]
    pub provenance: Option<Provenance>,
}

impl RdfDocument {
    /// Create a new RDF document from JSON-LD string
    ///
    /// Automatically normalizes entity names (e.g., "Alan Bean" -> "`Alan_Bean`")
    /// to match WebNLG/Wikidata conventions.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid
    pub fn from_json(json: &str) -> Result<Self> {
        let mut value: Value = serde_json::from_str(json)?;
        crate::normalize::normalize_jsonld_value(&mut value);
        Self::from_value(value)
    }

    /// Create a new RDF document from a `serde_json::Value`
    ///
    /// Automatically normalizes entity names for consistency.
    ///
    /// # Errors
    ///
    /// Returns an error if the value doesn't have required @context
    pub fn from_value(mut value: Value) -> Result<Self> {
        if !value.is_object() {
            return Err(Error::InvalidRdf(
                "RDF document must be a JSON object".to_string(),
            ));
        }

        // Normalize entity names before processing
        crate::normalize::normalize_jsonld_value(&mut value);

        let context = value
            .get("@context")
            .ok_or_else(|| Error::MissingField("@context".to_string()))?
            .clone();

        Ok(Self {
            context,
            data: value,
            provenance: None,
        })
    }

    /// Validate the RDF document structure
    ///
    /// # Errors
    ///
    /// Returns an error if validation fails
    pub fn validate(&self) -> Result<()> {
        // Ensure @context exists
        if self.context.is_null() {
            return Err(Error::Validation(
                "Missing or null @context field".to_string(),
            ));
        }

        // Ensure @type exists in the document
        if let Some(obj) = self.data.as_object() {
            if !obj.contains_key("@type") {
                return Err(Error::Validation("Missing @type field".to_string()));
            }
        }

        Ok(())
    }

    /// Convert to JSON string
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string_pretty(self).map_err(Error::from)
    }

    /// Get the entity type
    #[must_use]
    pub fn get_type(&self) -> Option<&str> {
        self.data.get("@type")?.as_str()
    }

    /// Get the entity ID
    #[must_use]
    pub fn get_id(&self) -> Option<&str> {
        self.data.get("@id")?.as_str()
    }

    /// Get a property value
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.data.get(key)
    }

    /// Enrich the document with canonical URIs from entity linking
    ///
    /// Updates the `@id` field with the canonical URI if available
    pub fn enrich_with_uri(&mut self, uri: impl Into<String>) {
        if let Some(obj) = self.data.as_object_mut() {
            obj.insert("@id".to_string(), Value::String(uri.into()));
        }
    }

    /// Set provenance metadata for this document
    pub fn set_provenance(&mut self, provenance: Provenance) {
        self.provenance = Some(provenance);
    }

    /// Get provenance metadata
    #[must_use]
    pub fn get_provenance(&self) -> Option<&Provenance> {
        self.provenance.as_ref()
    }

    /// Convert to JSON with provenance metadata included
    ///
    /// # Errors
    ///
    /// Returns an error if serialization fails
    pub fn to_json_with_provenance(&self) -> Result<String> {
        if let Some(prov) = &self.provenance {
            let mut output = serde_json::Map::new();

            // Add the main document data
            if let Some(obj) = self.data.as_object() {
                for (key, value) in obj {
                    output.insert(key.clone(), value.clone());
                }
            }

            // Add provenance metadata
            let mut prov_obj = serde_json::Map::new();
            if let Some((start, end)) = prov.text_span {
                prov_obj.insert("textSpan".to_string(), serde_json::json!({"start": start, "end": end}));
            }
            if let Some(conf) = prov.confidence {
                prov_obj.insert("confidence".to_string(), serde_json::json!(conf));
            }
            if let Some(chunk_id) = prov.chunk_id {
                prov_obj.insert("chunkId".to_string(), serde_json::json!(chunk_id));
            }
            if let Some(method) = &prov.method {
                prov_obj.insert("method".to_string(), serde_json::json!(method));
            }
            if let Some(source) = &prov.source_text {
                prov_obj.insert("sourceText".to_string(), serde_json::json!(source));
            }

            if !prov_obj.is_empty() {
                output.insert("_provenance".to_string(), serde_json::Value::Object(prov_obj));
            }

            serde_json::to_string_pretty(&output).map_err(Error::from)
        } else {
            self.to_json()
        }
    }

    /// Inject hardcoded JSON-LD @context to ensure correct URIs
    ///
    /// This replaces whatever @context the LLM generated with our hardcoded
    /// context.jsonld, preventing URI hallucinations and ensuring Schema.org compliance.
    ///
    /// # Errors
    ///
    /// Returns an error if the hardcoded context cannot be parsed
    pub fn inject_hardcoded_context(&mut self) -> Result<()> {
        let context_value: Value = serde_json::from_str(HARDCODED_CONTEXT)
            .map_err(|e| Error::Config(format!("Failed to parse hardcoded context: {e}")))?;

        self.context = context_value
            .get("@context")
            .ok_or_else(|| Error::Config("Hardcoded context missing @context field".to_string()))?
            .clone();

        // Update the @context in the data object as well
        if let Some(obj) = self.data.as_object_mut() {
            obj.insert("@context".to_string(), self.context.clone());
        }

        Ok(())
    }

    /// Create a new RDF document with hardcoded context injection
    ///
    /// This is the recommended way to create RDF documents from LLM output,
    /// as it ensures correct Schema.org URIs regardless of what the LLM generates.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid or context injection fails
    pub fn from_json_with_injected_context(json: &str) -> Result<Self> {
        let mut doc = Self::from_json(json)?;
        doc.inject_hardcoded_context()?;
        Ok(doc)
    }

    /// Get the entity name
    #[must_use]
    pub fn get_name(&self) -> Option<&str> {
        self.data.get("name")?.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_rdf_entity_builder() {
        let entity = RdfEntity::new(EntityType::Person)
            .with_id("https://example.org/person/test")
            .with_name("Test Person");

        assert_eq!(entity.entity_type, EntityType::Person);
        assert_eq!(
            entity.id,
            Some("https://example.org/person/test".to_string())
        );
        assert_eq!(entity.name, Some("Test Person".to_string()));
    }

    #[test]
    fn test_rdf_document_from_json() {
        let json = r#"{
            "@context": "https://schema.org/",
            "@type": "Person",
            "name": "Test"
        }"#;

        let doc = RdfDocument::from_json(json).unwrap();
        assert_eq!(doc.get_type(), Some("Person"));
    }

    #[test]
    fn test_rdf_document_validation() {
        let valid = json!({
            "@context": "https://schema.org/",
            "@type": "Person",
            "name": "Test"
        });

        let doc = RdfDocument::from_value(valid).unwrap();
        assert!(doc.validate().is_ok());

        let invalid = json!({
            "@context": "https://schema.org/",
            "name": "Test"
        });

        let doc = RdfDocument::from_value(invalid).unwrap();
        assert!(doc.validate().is_err());
    }
}
