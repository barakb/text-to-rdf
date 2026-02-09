//! Core RDF types and structures

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

use crate::{Error, Result};

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

/// An RDF document containing entities in JSON-LD format
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdfDocument {
    #[serde(rename = "@context")]
    pub context: Value,

    #[serde(flatten)]
    pub data: Value,
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
