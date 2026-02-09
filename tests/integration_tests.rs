//! Integration tests for the RDF extraction library

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use text_to_rdf::normalize::normalize_predicate;

#[derive(Debug, Deserialize)]
struct TestCase {
    id: String,
    raw_text: String,
    expected_triples: Vec<Triple>,
    expected_jsonld: Value,
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
struct Triple {
    subject: String,
    predicate: String,
    object: String,
}

#[derive(Debug)]
struct EvaluationMetrics {
    precision: f64,
    recall: f64,
    f1_score: f64,
    true_positives: usize,
    false_positives: usize,
    false_negatives: usize,
}

impl EvaluationMetrics {
    fn new(predicted: &HashSet<Triple>, expected: &HashSet<Triple>) -> Self {
        let true_positives = predicted.intersection(expected).count();
        let false_positives = predicted.len() - true_positives;
        let false_negatives = expected.len() - true_positives;

        let precision = if predicted.is_empty() {
            0.0
        } else {
            true_positives as f64 / predicted.len() as f64
        };

        let recall = if expected.is_empty() {
            0.0
        } else {
            true_positives as f64 / expected.len() as f64
        };

        let f1_score = if precision + recall == 0.0 {
            0.0
        } else {
            2.0 * (precision * recall) / (precision + recall)
        };

        Self {
            precision,
            recall,
            f1_score,
            true_positives,
            false_positives,
            false_negatives,
        }
    }
}

/// Extract triples from a JSON-LD document for comparison
fn extract_triples_from_jsonld(jsonld: &Value) -> HashSet<Triple> {
    let mut triples = HashSet::new();

    if let Some(obj) = jsonld.as_object() {
        let subject = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Extract simple properties
        for (key, value) in obj.iter() {
            if key.starts_with('@') || key == "name" {
                continue;
            }

            match value {
                Value::String(s) => {
                    triples.insert(Triple {
                        subject: subject.clone(),
                        predicate: normalize_predicate(key),
                        object: s.clone(),
                    });
                }
                Value::Object(_) => {
                    // Handle nested objects (like birthPlace)
                    if let Some(nested_name) = value.get("name").and_then(|v| v.as_str()) {
                        triples.insert(Triple {
                            subject: subject.clone(),
                            predicate: normalize_predicate(key),
                            object: nested_name.to_string(),
                        });
                    }
                }
                _ => {}
            }
        }
    }

    triples
}

#[test]
fn test_load_test_cases() {
    let test_cases_path = "tests/fixtures/test_cases.json";
    let contents = fs::read_to_string(test_cases_path)
        .expect("Should be able to read test cases file");

    let test_cases: Vec<TestCase> =
        serde_json::from_str(&contents).expect("Should be able to parse test cases JSON");

    assert!(!test_cases.is_empty(), "Should have at least one test case");
    assert_eq!(test_cases[0].id, "astronaut_birthdate_1");
}

#[test]
fn test_triple_extraction() {
    let jsonld = serde_json::json!({
        "@context": "https://schema.org/",
        "@type": "Person",
        "name": "Alan Bean",
        "birthDate": "1932-03-15"
    });

    let triples = extract_triples_from_jsonld(&jsonld);

    assert_eq!(triples.len(), 1);
    assert!(triples.contains(&Triple {
        subject: "Alan Bean".to_string(),
        predicate: "birthdat".to_string(), // Stemmed from "birthDate"
        object: "1932-03-15".to_string(),
    }));
}

#[test]
fn test_evaluation_metrics() {
    let predicted: HashSet<Triple> = [
        Triple {
            subject: "Person1".to_string(),
            predicate: "birthdat".to_string(), // Stemmed
            object: "1980".to_string(),
        },
        Triple {
            subject: "Person1".to_string(),
            predicate: "name".to_string(),
            object: "John".to_string(),
        },
    ]
    .iter()
    .cloned()
    .collect();

    let expected: HashSet<Triple> = [
        Triple {
            subject: "Person1".to_string(),
            predicate: "birthdat".to_string(), // Stemmed
            object: "1980".to_string(),
        },
        Triple {
            subject: "Person1".to_string(),
            predicate: "birthplac".to_string(), // Stemmed
            object: "NYC".to_string(),
        },
    ]
    .iter()
    .cloned()
    .collect();

    let metrics = EvaluationMetrics::new(&predicted, &expected);

    assert_eq!(metrics.true_positives, 1);
    assert_eq!(metrics.false_positives, 1);
    assert_eq!(metrics.false_negatives, 1);
    assert_eq!(metrics.precision, 0.5);
    assert_eq!(metrics.recall, 0.5);
    assert_eq!(metrics.f1_score, 0.5);
}

#[test]
fn test_perfect_match() {
    let triples: HashSet<Triple> = [Triple {
        subject: "Test".to_string(),
        predicate: "prop".to_string(),
        object: "value".to_string(),
    }]
    .iter()
    .cloned()
    .collect();

    let metrics = EvaluationMetrics::new(&triples, &triples);

    assert_eq!(metrics.precision, 1.0);
    assert_eq!(metrics.recall, 1.0);
    assert_eq!(metrics.f1_score, 1.0);
}

/// Integration test that would use the actual extractor
/// This test is ignored by default because it requires API keys
#[test]
#[ignore]
fn test_end_to_end_extraction() {
    use text_to_rdf::{ExtractionConfig, GenAiExtractor, RdfExtractor};

    let test_cases_path = "tests/fixtures/test_cases.json";
    let contents = fs::read_to_string(test_cases_path)
        .expect("Should be able to read test cases file");

    let test_cases: Vec<TestCase> =
        serde_json::from_str(&contents).expect("Should be able to parse test cases JSON");

    // Load config from .env file
    let config = ExtractionConfig::from_env().expect("Should load config from .env");
    let extractor = GenAiExtractor::new(config).expect("Should create extractor");

    let runtime = tokio::runtime::Runtime::new().unwrap();

    for test_case in test_cases.iter().take(1) {
        // Test first case only
        println!("Testing: {}", test_case.id);
        println!("Input: {}", test_case.raw_text);

        let result = runtime
            .block_on(extractor.extract(&test_case.raw_text))
            .expect("Extraction should succeed");

        // Extract triples from both
        let predicted_triples = extract_triples_from_jsonld(&result.data);
        let expected_triples: HashSet<Triple> =
            test_case.expected_triples.iter().cloned().collect();

        let metrics = EvaluationMetrics::new(&predicted_triples, &expected_triples);

        println!("Precision: {:.2}", metrics.precision);
        println!("Recall: {:.2}", metrics.recall);
        println!("F1 Score: {:.2}", metrics.f1_score);

        // We expect reasonable performance
        assert!(
            metrics.f1_score > 0.5,
            "F1 score should be above 0.5 for basic cases"
        );
    }
}

#[test]
fn test_jsonld_validation() {
    use text_to_rdf::RdfDocument;

    let test_cases_path = "tests/fixtures/test_cases.json";
    let contents = fs::read_to_string(test_cases_path)
        .expect("Should be able to read test cases file");

    let test_cases: Vec<TestCase> =
        serde_json::from_str(&contents).expect("Should be able to parse test cases JSON");

    for test_case in &test_cases {
        let doc = RdfDocument::from_value(test_case.expected_jsonld.clone())
            .expect("Expected JSON-LD should be valid");

        doc.validate()
            .expect("Expected JSON-LD should pass validation");

        assert!(doc.get_type().is_some(), "Should have a type");
    }
}

/// Integration test for entity linking with DBpedia
/// This test is ignored by default because it requires network access
#[test]
#[ignore]
fn test_entity_linking_integration() {
    use text_to_rdf::{EntityLinker, ExtractionConfig, GenAiExtractor, RdfExtractor, LinkingStrategy};

    let config = ExtractionConfig::from_env().expect("Should load config from .env");

    // Ensure entity linking is enabled
    assert!(config.entity_linker.enabled, "Entity linking should be enabled in .env");
    assert_eq!(config.entity_linker.strategy, LinkingStrategy::DbpediaSpotlight);

    let extractor = GenAiExtractor::new(config.clone()).expect("Should create extractor");
    let runtime = tokio::runtime::Runtime::new().unwrap();

    let text = "Alan Bean was born on March 15, 1932. He was an American astronaut.";

    println!("Testing entity linking with text: {}", text);

    // Stage 1: Extract with LLM
    let mut result = runtime
        .block_on(extractor.extract(text))
        .expect("Extraction should succeed");

    println!("Initial extraction:");
    println!("{}", result.to_json().unwrap());

    // Stage 2: Entity Linking
    let linker = EntityLinker::new(config.entity_linker).expect("Should create entity linker");

    if let Some(name) = result.get_name() {
        println!("\nAttempting to link entity: {}", name);

        let linked = runtime
            .block_on(linker.link_entity(text, name, result.get_type()))
            .expect("Entity linking should not error");

        if let Some(entity) = linked {
            println!("✓ Successfully linked to: {}", entity.uri);
            println!("  Confidence: {:.2}", entity.confidence);
            println!("  Types: {:?}", entity.types);

            // Verify URI format
            assert!(entity.uri.starts_with("http://") || entity.uri.starts_with("https://"));
            assert!(entity.uri.contains("dbpedia.org"));
            assert!(entity.confidence >= 0.5);

            // Enrich the document
            result.enrich_with_uri(entity.uri.clone());

            // Verify the URI was added
            assert_eq!(result.get_id(), Some(entity.uri.as_str()));

            println!("\nEnriched JSON-LD:");
            println!("{}", result.to_json().unwrap());
        } else {
            println!("✗ No entity link found (confidence too low or service unavailable)");
            println!("Note: This may be expected if DBpedia Spotlight is down or the entity is uncommon");
        }
    } else {
        panic!("Extracted result should have a name property");
    }
}

/// Test validation with entity linking
#[test]
fn test_validation_with_linking() {
    use text_to_rdf::{RdfDocument, RdfValidator};
    use serde_json::json;

    // Document with canonical URI (as if from entity linking)
    let doc = RdfDocument::from_value(json!({
        "@context": "https://schema.org/",
        "@type": "Person",
        "@id": "http://dbpedia.org/resource/Alan_Bean",
        "name": "alan_bean",
        "birthDate": "1932-03-15"
    }))
    .unwrap();

    let validator = RdfValidator::with_schema_org_rules();
    let result = validator.validate(&doc);

    assert!(result.is_valid());

    // Should have proper URI
    assert!(doc.get_id().is_some());
    assert!(doc.get_id().unwrap().contains("dbpedia.org"));
}
