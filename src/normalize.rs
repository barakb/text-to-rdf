//! Text normalization utilities for RDF entity names and predicates
//!
//! - Entity names: Uses `slug` crate for robust Unicode handling
//! - Predicates: Uses `rust-stemmers` for relation normalization (e.g., "runs"/"running" → "run")

use rust_stemmers::{Algorithm, Stemmer};
use slug::slugify;

/// Normalize an entity name for consistent RDF representation
///
/// Converts text to a normalized form using proper Unicode handling:
/// - Transliterates Unicode characters to ASCII
/// - Replaces spaces and special characters with underscores
/// - Handles accents, diacritics, and non-ASCII characters
/// - Matches WebNLG/Wikidata entity naming conventions
///
/// # Examples
///
/// ```
/// use text_to_rdf::normalize::normalize_entity_name;
///
/// assert_eq!(normalize_entity_name("Alan Bean"), "alan_bean");
/// assert_eq!(normalize_entity_name("José García"), "jose_garcia");
/// assert_eq!(normalize_entity_name("MIT"), "mit");
/// ```
#[must_use]
pub fn normalize_entity_name(name: &str) -> String {
    // Use slug crate for proper Unicode normalization
    // Replace hyphens with underscores to match RDF conventions
    slugify(name).replace('-', "_")
}

/// Normalize a predicate/relation name using stemming
///
/// Stems the predicate to its root form so that variations map to the same relation:
/// - "runs", "running", "ran" → "run"
/// - "graduates", "graduating", "graduated" → "graduat"
/// - "serves", "serving", "served" → "serv"
///
/// Uses Porter stemming algorithm for English text.
///
/// # Examples
///
/// ```
/// use text_to_rdf::normalize::normalize_predicate;
///
/// assert_eq!(normalize_predicate("runs"), "run");
/// assert_eq!(normalize_predicate("running"), "run");
/// assert_eq!(normalize_predicate("graduated"), "graduat");
/// ```
#[must_use]
pub fn normalize_predicate(predicate: &str) -> String {
    let stemmer = Stemmer::create(Algorithm::English);

    // Convert to lowercase and stem
    let normalized = predicate.to_lowercase();

    // If it's a camelCase predicate (like "birthDate"), split and stem each word
    if normalized.chars().any(char::is_uppercase) {
        // Split on uppercase boundaries
        let words = split_camel_case(&normalized);
        words
            .iter()
            .map(|w| stemmer.stem(w).into_owned())
            .collect::<Vec<_>>()
            .join("_")
    } else {
        // Simple predicate, just stem it
        stemmer.stem(&normalized).to_string()
    }
}

/// Split `camelCase` or `PascalCase` into words
fn split_camel_case(s: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();

    for ch in s.chars() {
        if ch.is_uppercase() && !current.is_empty() {
            words.push(current.clone());
            current.clear();
        }
        current.push(ch.to_lowercase().next().unwrap());
    }

    if !current.is_empty() {
        words.push(current);
    }

    words
}

/// Normalize a JSON-LD value by recursively processing all string fields
///
/// **Important**: Only normalizes the root entity's name field, not nested entities.
/// Nested entity names (like organization names in alumniOf) are preserved for
/// accurate matching against external knowledge bases.
pub fn normalize_jsonld_value(value: &mut serde_json::Value) {
    normalize_jsonld_value_impl(value, true);
}

fn normalize_jsonld_value_impl(value: &mut serde_json::Value, is_root: bool) {
    use serde_json::Value;

    match value {
        Value::String(s) => {
            // Don't normalize URLs, dates, or @context values
            if !s.starts_with("http")
                && !s.contains("://")
                && !s.contains('-')
                && s.chars().any(char::is_whitespace)
                && s.chars().filter(|c| c.is_uppercase()).count() > 0
            {
                // Likely a proper name with spaces
                *s = normalize_entity_name(s);
            }
        }
        Value::Object(map) => {
            // Only normalize the root entity's name field
            // Preserve nested entity names for external KB matching
            if is_root {
                if let Some(Value::String(name)) = map.get_mut("name") {
                    let normalized = normalize_entity_name(name);
                    *name = normalized;
                }
            }

            // Recursively process nested objects (but mark them as non-root)
            for (key, val) in map.iter_mut() {
                if key != "@context" && key != "@id" && key != "@type" && key != "name" {
                    normalize_jsonld_value_impl(val, false);
                }
            }
        }
        Value::Array(arr) => {
            for item in arr.iter_mut() {
                normalize_jsonld_value_impl(item, false);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_entity_name() {
        assert_eq!(normalize_entity_name("Alan Bean"), "alan_bean");
        assert_eq!(normalize_entity_name("Albert Einstein"), "albert_einstein");
        assert_eq!(normalize_entity_name("MIT"), "mit");
        assert_eq!(normalize_entity_name("New York"), "new_york");
    }

    #[test]
    fn test_normalize_unicode() {
        // Test Unicode handling
        assert_eq!(normalize_entity_name("José García"), "jose_garcia");
        assert_eq!(
            normalize_entity_name("Björk Guðmundsdóttir"),
            "bjork_gudmundsdottir"
        );
        assert_eq!(normalize_entity_name("Cañón City"), "canon_city");
    }

    #[test]
    fn test_normalize_special_chars() {
        // Test special character handling
        assert_eq!(
            normalize_entity_name("AT&T Corporation"),
            "at_t_corporation"
        );
        assert_eq!(normalize_entity_name("O'Reilly Media"), "o_reilly_media");
    }

    #[test]
    fn test_normalize_predicate() {
        // Test verb stemming - different forms should map to same stem
        assert_eq!(normalize_predicate("runs"), "run");
        assert_eq!(normalize_predicate("running"), "run");
        assert_eq!(normalize_predicate("ran"), "ran"); // irregular verb, stays as-is

        assert_eq!(normalize_predicate("serves"), "serv");
        assert_eq!(normalize_predicate("serving"), "serv");
        assert_eq!(normalize_predicate("served"), "serv");

        assert_eq!(normalize_predicate("graduates"), "graduat");
        assert_eq!(normalize_predicate("graduating"), "graduat");
        assert_eq!(normalize_predicate("graduated"), "graduat");
    }

    #[test]
    fn test_normalize_predicate_camelcase() {
        // Test camelCase predicates (common in Schema.org)
        assert_eq!(normalize_predicate("birthdate"), "birthdat");
        assert_eq!(normalize_predicate("almamater"), "almamat");
        assert_eq!(normalize_predicate("cityserved"), "cityserv");
    }

    #[test]
    fn test_normalize_jsonld() {
        use serde_json::json;

        let mut value = json!({
            "@context": "https://schema.org/",
            "@type": "Person",
            "name": "Alan Bean",
            "birthDate": "1932-03-15"
        });

        normalize_jsonld_value(&mut value);

        assert_eq!(value["name"], "alan_bean");
        assert_eq!(value["birthDate"], "1932-03-15"); // Dates not normalized
        assert_eq!(value["@context"], "https://schema.org/"); // URLs not normalized
    }

    #[test]
    fn test_normalize_jsonld_preserves_nested_names() {
        use serde_json::json;

        let mut value = json!({
            "@context": "https://schema.org/",
            "@type": "Person",
            "name": "Alan Bean",
            "alumniOf": {
                "@type": "EducationalOrganization",
                "name": "UT Austin"
            }
        });

        normalize_jsonld_value(&mut value);

        // Root entity name should be normalized
        assert_eq!(value["name"], "alan_bean");

        // Nested entity names should be PRESERVED (not normalized)
        assert_eq!(value["alumniOf"]["name"], "UT Austin");
    }

    #[test]
    fn test_normalize_jsonld_location() {
        use serde_json::json;

        let mut value = json!({
            "@context": "https://schema.org/",
            "@type": "Airport",
            "name": "Aarhus Airport",
            "location": {
                "@type": "Place",
                "name": "Aarhus",
                "addressCountry": "Denmark"
            }
        });

        normalize_jsonld_value(&mut value);

        // Root entity name normalized
        assert_eq!(value["name"], "aarhus_airport");

        // Nested location name preserved
        assert_eq!(value["location"]["name"], "Aarhus");
        assert_eq!(value["location"]["addressCountry"], "Denmark");
    }
}
