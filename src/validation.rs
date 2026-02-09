//! RDF Validation Module - Stage 5 of the extraction pipeline
//!
//! Provides SHACL-like validation to ensure extracted RDF data is semantically sound
//! before committing to the knowledge graph.

use crate::types::RdfDocument;
use serde_json::Value;

/// Validation rules for RDF documents
#[derive(Debug, Clone)]
pub struct ValidationRule {
    pub name: String,
    pub description: String,
    pub required_properties: Vec<String>,
    pub entity_type: Option<String>,
}

/// Validation result with detailed feedback
#[derive(Debug, Clone)]
pub struct ValidationResult {
    pub valid: bool,
    pub violations: Vec<Violation>,
}

/// A validation violation
#[derive(Debug, Clone)]
pub struct Violation {
    pub rule: String,
    pub message: String,
    pub severity: Severity,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// RDF validator with configurable rules
pub struct RdfValidator {
    rules: Vec<ValidationRule>,
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
        Self { rules: Vec::new() }
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
        });

        // Rule: Organization must have a name
        validator.add_rule(ValidationRule {
            name: "organization_requires_name".to_string(),
            description: "An Organization entity must have a 'name' property".to_string(),
            required_properties: vec!["name".to_string()],
            entity_type: Some("Organization".to_string()),
        });

        // Rule: Place must have a name
        validator.add_rule(ValidationRule {
            name: "place_requires_name".to_string(),
            description: "A Place entity must have a 'name' property".to_string(),
            required_properties: vec!["name".to_string()],
            entity_type: Some("Place".to_string()),
        });

        validator
    }

    /// Add a validation rule
    pub fn add_rule(&mut self, rule: ValidationRule) {
        self.rules.push(rule);
    }

    /// Validate an RDF document
    #[must_use]
    pub fn validate(&self, document: &RdfDocument) -> ValidationResult {
        let mut violations = Vec::new();

        // Basic structure validation
        if let Err(e) = document.validate() {
            violations.push(Violation {
                rule: "basic_structure".to_string(),
                message: format!("Basic validation failed: {e}"),
                severity: Severity::Error,
            });
            return ValidationResult {
                valid: false,
                violations,
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
                    violations.push(Violation {
                        rule: rule.name.clone(),
                        message: format!(
                            "Missing required property '{required_prop}': {}",
                            rule.description
                        ),
                        severity: Severity::Error,
                    });
                }
            }
        }

        // Validate dates if present
        if let Some(birth_date) = document.get("birthDate") {
            if !Self::is_valid_date(birth_date) {
                violations.push(Violation {
                    rule: "valid_date_format".to_string(),
                    message: "birthDate must be in ISO 8601 format (YYYY-MM-DD)".to_string(),
                    severity: Severity::Warning,
                });
            }
        }

        // Validate URLs if present
        if let Some(id) = document.get_id() {
            if !Self::is_valid_url(id) {
                violations.push(Violation {
                    rule: "valid_uri".to_string(),
                    message: "@id must be a valid URI".to_string(),
                    severity: Severity::Warning,
                });
            }
        }

        ValidationResult {
            valid: violations.iter().all(|v| v.severity != Severity::Error),
            violations,
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
