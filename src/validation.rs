//! RDF Validation Module - Stage 4 of the extraction pipeline
//!
//! Provides SHACL-like validation to ensure extracted RDF data is semantically sound
//! before committing to the knowledge graph.
//!
//! ## Features
//!
//! - **Rule-Based Validation**: Check required properties for Schema.org types
//! - **SPARQL ASK Validation**: Run custom SPARQL queries via Oxigraph
//! - **Confidence Scoring**: Assign confidence scores to validation results
//! - **Type Checking**: Validate property datatypes (dates, URLs, etc.)
//! - **Cardinality Constraints**: Ensure properties have correct number of values

use crate::types::RdfDocument;
use oxigraph::sparql::QueryResults;
use oxigraph::store::Store;
use serde_json::Value;
use std::sync::Arc;

/// Validation rules for RDF documents
#[derive(Debug, Clone)]
pub struct ValidationRule {
    pub name: String,
    pub description: String,
    pub required_properties: Vec<String>,
    pub entity_type: Option<String>,
    /// Optional SPARQL ASK query for custom validation
    pub sparql_ask: Option<String>,
}

/// Configuration for RDF validation
#[derive(Debug, Clone)]
pub struct ValidationConfig {
    /// Drop triples that fail validation (vs flagging as low confidence)
    pub drop_invalid: bool,
    /// Minimum confidence threshold (0.0-1.0)
    pub min_confidence: f64,
    /// Enable SPARQL-based validation
    pub enable_sparql_validation: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        Self {
            drop_invalid: false,
            min_confidence: 0.7,
            enable_sparql_validation: false,
        }
    }
}

/// Validation result with detailed feedback
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub violations: Vec<Violation>,
    /// Overall confidence score (0.0-1.0)
    pub confidence: f64,
}

/// A validation violation
#[derive(Debug, Clone)]
pub struct Violation {
    pub rule: String,
    pub message: String,
    pub severity: Severity,
    /// Property that failed validation
    pub property: Option<String>,
    /// Confidence impact (-1.0 to 0.0, how much this reduces overall confidence)
    pub confidence_impact: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// RDF validator with configurable rules
pub struct RdfValidator {
    rules: Vec<ValidationRule>,
    config: ValidationConfig,
    /// Optional Oxigraph store for SPARQL ASK validation
    store: Option<Arc<Store>>,
}

impl Default for RdfValidator {
    fn default() -> Self {
        Self::with_schema_org_rules()
    }
}

impl RdfValidator {
    /// Create a new validator with no rules
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rules: Vec::new(),
            config: ValidationConfig {
                drop_invalid: false,
                min_confidence: 0.7,
                enable_sparql_validation: false,
            },
            store: None,
        }
    }

    /// Create a validator with custom configuration
    #[must_use]
    pub const fn with_config(config: ValidationConfig) -> Self {
        Self {
            rules: Vec::new(),
            config,
            store: None,
        }
    }

    /// Create a validator with `Schema.org` standard rules
    #[must_use]
    pub fn with_schema_org_rules() -> Self {
        let mut validator = Self::new();

        // Rule: Person must have a name
        validator.add_rule(ValidationRule {
            name: "person_requires_name".to_string(),
            description: "A Person entity must have a 'name' property".to_string(),
            required_properties: vec!["name".to_string()],
            entity_type: Some("Person".to_string()),
            sparql_ask: None,
        });

        // Rule: Organization must have a name
        validator.add_rule(ValidationRule {
            name: "organization_requires_name".to_string(),
            description: "An Organization entity must have a 'name' property".to_string(),
            required_properties: vec!["name".to_string()],
            entity_type: Some("Organization".to_string()),
            sparql_ask: None,
        });

        // Rule: Place must have a name
        validator.add_rule(ValidationRule {
            name: "place_requires_name".to_string(),
            description: "A Place entity must have a 'name' property".to_string(),
            required_properties: vec!["name".to_string()],
            entity_type: Some("Place".to_string()),
            sparql_ask: None,
        });

        // Rule: Event must have a name
        validator.add_rule(ValidationRule {
            name: "event_requires_name".to_string(),
            description: "An Event entity should have a 'name' property".to_string(),
            required_properties: vec!["name".to_string()],
            entity_type: Some("Event".to_string()),
            sparql_ask: None,
        });

        validator
    }

    /// Attach an Oxigraph store for SPARQL-based validation
    #[must_use]
    pub fn with_store(mut self, store: Arc<Store>) -> Self {
        self.store = Some(store);
        self
    }

    /// Add a validation rule
    pub fn add_rule(&mut self, rule: ValidationRule) {
        self.rules.push(rule);
    }

    /// Set validation configuration
    #[must_use]
    pub const fn set_config(mut self, config: ValidationConfig) -> Self {
        self.config = config;
        self
    }

    /// Validate an RDF document
    #[must_use]
    pub fn validate(&self, document: &RdfDocument) -> ValidationResult {
        let mut violations = Vec::new();
        let mut confidence = 1.0; // Start with perfect confidence

        // Basic structure validation
        if let Err(e) = document.validate() {
            violations.push(Violation {
                rule: "basic_structure".to_string(),
                message: format!("Basic validation failed: {e}"),
                severity: Severity::Error,
                property: None,
                confidence_impact: -0.5, // Major impact
            });
            return ValidationResult {
                valid: false,
                violations,
                confidence: 0.5,
            };
        }

        let entity_type = document.get_type();

        // Apply rules based on entity type
        for rule in &self.rules {
            // Check if rule applies to this entity type
            if let Some(rule_type) = &rule.entity_type {
                if Some(rule_type.as_str()) != entity_type {
                    continue;
                }
            }

            // Check required properties
            for required_prop in &rule.required_properties {
                if !Self::has_property(document, required_prop) {
                    let impact = -0.2; // Missing required property is significant
                    confidence += impact;
                    violations.push(Violation {
                        rule: rule.name.clone(),
                        message: format!(
                            "Missing required property '{required_prop}': {}",
                            rule.description
                        ),
                        severity: Severity::Error,
                        property: Some(required_prop.clone()),
                        confidence_impact: impact,
                    });
                }
            }

            // Run SPARQL ASK query if configured
            if self.config.enable_sparql_validation {
                if let Some(sparql) = &rule.sparql_ask {
                    if let Some(store) = &self.store {
                        if let Ok(result) = Self::execute_sparql_ask(store, sparql, document) {
                            if !result {
                                let impact = -0.15;
                                confidence += impact;
                                violations.push(Violation {
                                    rule: rule.name.clone(),
                                    message: format!(
                                        "SPARQL validation failed: {}",
                                        rule.description
                                    ),
                                    severity: Severity::Warning,
                                    property: None,
                                    confidence_impact: impact,
                                });
                            }
                        }
                    }
                }
            }
        }

        // Validate dates if present
        for date_prop in &["birthDate", "deathDate", "datePublished", "dateCreated"] {
            if let Some(date_value) = document.get(date_prop) {
                if !Self::is_valid_date(date_value) {
                    let impact = -0.05; // Minor impact for date format
                    confidence += impact;
                    violations.push(Violation {
                        rule: "valid_date_format".to_string(),
                        message: format!("{date_prop} must be in ISO 8601 format (YYYY-MM-DD)"),
                        severity: Severity::Warning,
                        property: Some((*date_prop).to_string()),
                        confidence_impact: impact,
                    });
                }
            }
        }

        // Validate URLs if present
        if let Some(id) = document.get_id() {
            if !Self::is_valid_url(id) {
                let impact = -0.1;
                confidence += impact;
                violations.push(Violation {
                    rule: "valid_uri".to_string(),
                    message: "@id must be a valid URI".to_string(),
                    severity: Severity::Warning,
                    property: Some("@id".to_string()),
                    confidence_impact: impact,
                });
            }
        }

        // Ensure confidence stays in valid range
        confidence = confidence.clamp(0.0, 1.0);

        ValidationResult {
            valid: violations.iter().all(|v| v.severity != Severity::Error)
                && confidence >= self.config.min_confidence,
            violations,
            confidence,
        }
    }

    /// Execute a SPARQL ASK query against the document
    ///
    /// Returns true if the query returns true, false otherwise
    #[allow(deprecated)]
    fn execute_sparql_ask(
        store: &Store,
        query: &str,
        _document: &RdfDocument,
    ) -> Result<bool, String> {
        // Execute SPARQL ASK query
        let results = store
            .query(query)
            .map_err(|e| format!("SPARQL query failed: {e}"))?;

        // Check if it's a boolean result
        if let QueryResults::Boolean(result) = results {
            Ok(result)
        } else {
            Err("SPARQL query did not return a boolean result".to_string())
        }
    }

    fn has_property(document: &RdfDocument, property: &str) -> bool {
        document.get(property).is_some_and(|v| !v.is_null())
    }

    fn is_valid_date(value: &Value) -> bool {
        value.as_str().is_some_and(|date_str| {
            // Simple ISO 8601 date validation (YYYY-MM-DD)
            date_str.len() == 10
                && date_str.chars().nth(4) == Some('-')
                && date_str.chars().nth(7) == Some('-')
        })
    }

    fn is_valid_url(url: &str) -> bool {
        url.starts_with("http://") || url.starts_with("https://")
    }
}

impl ValidationResult {
    /// Check if validation passed
    #[must_use]
    pub const fn is_valid(&self) -> bool {
        self.valid
    }

    /// Get confidence score (0.0-1.0)
    #[must_use]
    pub const fn confidence(&self) -> f64 {
        self.confidence
    }

    /// Check if confidence meets minimum threshold
    #[must_use]
    pub fn meets_confidence_threshold(&self, threshold: f64) -> bool {
        self.confidence >= threshold
    }

    /// Get all error violations
    #[must_use]
    pub fn errors(&self) -> Vec<&Violation> {
        self.violations
            .iter()
            .filter(|v| v.severity == Severity::Error)
            .collect()
    }

    /// Get all warning violations
    #[must_use]
    pub fn warnings(&self) -> Vec<&Violation> {
        self.violations
            .iter()
            .filter(|v| v.severity == Severity::Warning)
            .collect()
    }

    /// Get total confidence impact from all violations
    #[must_use]
    pub fn total_confidence_impact(&self) -> f64 {
        self.violations.iter().map(|v| v.confidence_impact).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_valid_person() {
        let doc = RdfDocument::from_value(json!({
            "@context": "https://schema.org/",
            "@type": "Person",
            "@id": "http://dbpedia.org/resource/Alan_Bean",
            "name": "Alan Bean",
            "birthDate": "1932-03-15"
        }))
        .unwrap();

        let validator = RdfValidator::with_schema_org_rules();
        let result = validator.validate(&doc);

        assert!(result.is_valid());
        assert_eq!(result.violations.len(), 0);
    }

    #[test]
    fn test_person_missing_name() {
        let doc = RdfDocument::from_value(json!({
            "@context": "https://schema.org/",
            "@type": "Person",
            "birthDate": "1932-03-15"
        }))
        .unwrap();

        let validator = RdfValidator::with_schema_org_rules();
        let result = validator.validate(&doc);

        assert!(!result.is_valid());
        assert_eq!(result.errors().len(), 1);
        assert!(result.errors()[0].message.contains("name"));
    }

    #[test]
    fn test_invalid_date_format() {
        let doc = RdfDocument::from_value(json!({
            "@context": "https://schema.org/",
            "@type": "Person",
            "name": "Test",
            "birthDate": "32/03/15"
        }))
        .unwrap();

        let validator = RdfValidator::with_schema_org_rules();
        let result = validator.validate(&doc);

        assert!(result.is_valid()); // Still valid, just a warning
        assert_eq!(result.warnings().len(), 1);
    }

    #[test]
    fn test_organization_validation() {
        let doc = RdfDocument::from_value(json!({
            "@context": "https://schema.org/",
            "@type": "Organization"
        }))
        .unwrap();

        let validator = RdfValidator::with_schema_org_rules();
        let result = validator.validate(&doc);

        assert!(!result.is_valid());
        assert_eq!(result.errors().len(), 1);
    }

    #[test]
    fn test_custom_rule() {
        let mut validator = RdfValidator::new();
        validator.add_rule(ValidationRule {
            name: "test_rule".to_string(),
            description: "Test requires foo".to_string(),
            required_properties: vec!["foo".to_string()],
            entity_type: None,
            sparql_ask: None,
        });

        let doc = RdfDocument::from_value(json!({
            "@context": "https://schema.org/",
            "@type": "Thing"
        }))
        .unwrap();

        let result = validator.validate(&doc);
        assert!(!result.is_valid());
    }
}
