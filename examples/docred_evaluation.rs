//! `DocRED` Document-Level Relation Extraction Evaluation
//!
//! This example demonstrates document-level RDF extraction evaluation using the `DocRED` dataset:
//! 1. Extract RDF from multi-paragraph documents
//! 2. Test cross-sentence and inter-paragraph relation extraction
//! 3. Evaluate coreference resolution capabilities
//! 4. Compare against gold standard relations
//!
//! ## `DocRED` Dataset
//!
//! `DocRED` is a large-scale document-level relation extraction dataset with:
//! - 5,053 Wikipedia documents
//! - 132,375 entities with coreference information
//! - 56,354 relational facts
//! - Relations that span multiple sentences and paragraphs
//!
//! ## Running This Example
//!
//! With API key (recommended - document-level extraction needs strong reasoning):
//! ```bash
//! export GENAI_API_KEY="your-key"
//! export RDF_EXTRACTION_MODEL=claude-3-5-sonnet-20241022  # Recommended
//! cargo run --example docred_evaluation
//! ```
//!
//! With Ollama (use 70B model for better results):
//! ```bash
//! ollama serve
//! ollama pull llama3.3:70b
//! export RDF_EXTRACTION_MODEL=llama3.3:70b
//! cargo run --example docred_evaluation
//! ```

use serde::Deserialize;
use serde_json::Value;
use std::collections::HashSet;
use std::env;
use std::fs;
use text_to_rdf::normalize::normalize_predicate;
use text_to_rdf::{ExtractionConfig, GenAiExtractor};

/// A document from the `DocRED` dataset
#[derive(Debug, Deserialize)]
struct DocREDDocument {
    /// Document ID
    #[allow(dead_code)]
    id: String,

    /// Document title (usually entity name)
    title: String,

    /// Paragraphs of text (each paragraph is a Vec of sentences)
    #[serde(rename = "sents")]
    sentences: Vec<Vec<String>>,

    /// Named entities with coreference clusters
    #[serde(rename = "vertexSet")]
    entities: Vec<Vec<EntityMention>>,

    /// Gold standard relations
    labels: Vec<Relation>,
}

/// An entity mention in the document
#[derive(Debug, Deserialize, Clone)]
struct EntityMention {
    /// Entity name/text
    name: String,

    /// Sentence index (0-based)
    #[allow(dead_code)]
    sent_id: usize,

    /// Entity type (PER, ORG, LOC, etc.)
    #[serde(rename = "type")]
    #[allow(dead_code)]
    entity_type: String,

    /// Start position in sentence
    #[allow(dead_code)]
    pos: Vec<usize>,
}

/// A relation between two entities
#[derive(Debug, Deserialize, Clone, PartialEq, Eq, Hash)]
struct Relation {
    /// Head entity index (into entities array)
    #[serde(rename = "h")]
    head: usize,

    /// Tail entity index
    #[serde(rename = "t")]
    tail: usize,

    /// Relation type (P17, P131, etc. - Wikidata property IDs)
    #[serde(rename = "r")]
    relation_type: String,

    /// Evidence sentence IDs
    #[allow(dead_code)]
    evidence: Vec<usize>,
}

/// A simplified triple for comparison
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct Triple {
    subject: String,
    predicate: String,
    object: String,
}

/// Evaluation metrics for document-level extraction
#[derive(Debug)]
struct DocumentMetrics {
    precision: f64,
    recall: f64,
    f1_score: f64,
    true_positives: usize,
    false_positives: usize,
    false_negatives: usize,
    #[allow(dead_code)]
    total_sentences: usize,
    #[allow(dead_code)]
    total_entities: usize,
    #[allow(dead_code)]
    cross_sentence_relations: usize,
}

impl DocumentMetrics {
    fn new(
        predicted: &HashSet<Triple>,
        expected: &HashSet<Triple>,
        total_sentences: usize,
        total_entities: usize,
    ) -> Self {
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
            total_sentences,
            total_entities,
            cross_sentence_relations: 0, // Will be calculated separately
        }
    }
}

impl DocREDDocument {
    /// Get the full document text as a single string
    fn get_full_text(&self) -> String {
        let mut paragraphs = Vec::new();

        for paragraph_sentences in &self.sentences {
            let paragraph = paragraph_sentences.join(" ");
            paragraphs.push(paragraph);
        }

        paragraphs.join("\n\n")
    }

    /// Get the canonical name for an entity (first mention)
    fn get_entity_name(&self, entity_idx: usize) -> Option<String> {
        self.entities
            .get(entity_idx)
            .and_then(|mentions| mentions.first())
            .map(|mention| mention.name.clone())
    }

    /// Count total sentences in document
    fn sentence_count(&self) -> usize {
        self.sentences.iter().map(std::vec::Vec::len).sum()
    }
}

/// Map Wikidata property IDs to Schema.org properties
fn map_wikidata_to_schema(property_id: &str) -> Option<&'static str> {
    match property_id {
        "P17" => Some("addressCountry"),    // country
        "P131" => Some("containedInPlace"), // located in administrative unit
        "P276" => Some("location"),         // location
        "P27" => Some("nationality"),       // country of citizenship
        "P69" => Some("alumniOf"),          // educated at
        "P108" => Some("worksFor"),         // employer
        "P39" => Some("jobTitle"),          // position held
        "P102" => Some("memberOf"),         // member of political party
        "P54" => Some("memberOf"),          // member of sports team
        "P463" => Some("memberOf"),         // member of
        "P19" => Some("birthPlace"),        // place of birth
        "P20" => Some("deathPlace"),        // place of death
        "P569" => Some("birthDate"),        // date of birth
        "P570" => Some("deathDate"),        // date of death
        "P571" => Some("foundingDate"),     // inception
        "P576" => Some("dissolutionDate"),  // dissolved
        "P37" => Some("language"),          // official language
        "P159" => Some("location"),         // headquarters location
        _ => None,
    }
}

/// Convert `DocRED` relations to normalized triples
fn docred_to_triples(doc: &DocREDDocument) -> HashSet<Triple> {
    let mut triples = HashSet::new();

    for relation in &doc.labels {
        if let (Some(subject), Some(object)) = (
            doc.get_entity_name(relation.head),
            doc.get_entity_name(relation.tail),
        ) {
            if let Some(schema_property) = map_wikidata_to_schema(&relation.relation_type) {
                // Normalize: lowercase and replace spaces with underscores
                // Preserve trailing punctuation like periods in "Inc.", "Ltd.", etc.
                let normalized_subject = subject.to_lowercase().replace(' ', "_");

                triples.insert(Triple {
                    subject: normalized_subject,
                    predicate: normalize_predicate(schema_property),
                    object,
                });
            }
        }
    }

    triples
}

/// Extract triples from JSON-LD output
fn extract_triples_from_jsonld(jsonld: &Value) -> HashSet<Triple> {
    let mut triples = HashSet::new();

    if let Some(obj) = jsonld.as_object() {
        let subject = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        // Normalize subject to match expected format
        let normalized_subject = subject.to_lowercase().replace(' ', "_");

        for (key, value) in obj {
            if key.starts_with('@') || key == "name" {
                continue;
            }

            match value {
                Value::String(s) => {
                    triples.insert(Triple {
                        subject: normalized_subject.clone(),
                        predicate: normalize_predicate(key),
                        object: s.clone(),
                    });
                }
                Value::Object(nested) => {
                    // Handle nested entities - preserve original name
                    if let Some(nested_name) = nested.get("name").and_then(|v| v.as_str()) {
                        triples.insert(Triple {
                            subject: normalized_subject.clone(),
                            predicate: normalize_predicate(key),
                            object: nested_name.to_string(),
                        });
                    }

                    // Handle nested properties (e.g., location.addressCountry)
                    for (nested_key, nested_value) in nested {
                        if nested_key.starts_with('@') || nested_key == "name" {
                            continue;
                        }
                        if let Some(s) = nested_value.as_str() {
                            // Extract nested property as a direct triple
                            triples.insert(Triple {
                                subject: normalized_subject.clone(),
                                predicate: normalize_predicate(nested_key),
                                object: s.to_string(),
                            });
                        }
                    }
                }
                Value::Array(arr) => {
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            triples.insert(Triple {
                                subject: normalized_subject.clone(),
                                predicate: normalize_predicate(key),
                                object: s.to_string(),
                            });
                        } else if let Some(obj) = item.as_object() {
                            if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                                triples.insert(Triple {
                                    subject: normalized_subject.clone(),
                                    predicate: normalize_predicate(key),
                                    object: name.to_string(),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    triples
}

/// Filter out likely incorrect triples based on heuristics
fn filter_likely_incorrect_triples(triples: HashSet<Triple>) -> HashSet<Triple> {
    triples
        .into_iter()
        .filter(|triple| {
            let predicate = &triple.predicate;

            // Only filter out clearly wrong properties that shouldn't exist
            // Be very conservative - only remove obvious mistakes

            // Filter founder/funder - we expect worksFor instead
            if predicate.contains("founder") || predicate.contains("funder") {
                return false;
            }

            // Filter currentceo/ceo - not in our Schema.org mapping
            if predicate.contains("currentceo") || predicate == "ceo" {
                return false;
            }

            // Filter alumni (reverse) - we expect alumniOf instead
            if predicate.contains("alumni") && !predicate.contains("alumniof") {
                return false;
            }

            // Everything else passes through
            true
        })
        .collect()
}

/// Check if Ollama is available
fn is_ollama_available() -> bool {
    use std::net::TcpStream;
    use std::time::Duration;

    TcpStream::connect_timeout(
        &"127.0.0.1:11434".parse().unwrap(),
        Duration::from_millis(500),
    )
    .is_ok()
}

/// Print document analysis
fn print_document_info(doc: &DocREDDocument) {
    println!("\n📄 Document: {}", doc.title);
    println!("   Sentences: {}", doc.sentence_count());
    println!("   Entities: {}", doc.entities.len());
    println!("   Relations: {}", doc.labels.len());

    // Count cross-sentence relations
    let cross_sentence = doc
        .labels
        .iter()
        .filter(|r| {
            if let (Some(head_mentions), Some(tail_mentions)) =
                (doc.entities.get(r.head), doc.entities.get(r.tail))
            {
                if let (Some(h), Some(t)) = (head_mentions.first(), tail_mentions.first()) {
                    return h.sent_id != t.sent_id;
                }
            }
            false
        })
        .count();

    println!(
        "   Cross-sentence relations: {}/{}",
        cross_sentence,
        doc.labels.len()
    );
}

/// Print evaluation report
fn print_evaluation_report(
    doc: &DocREDDocument,
    predicted: &HashSet<Triple>,
    expected: &HashSet<Triple>,
    metrics: &DocumentMetrics,
) {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("Document: {}", doc.title);
    println!("═══════════════════════════════════════════════════════════════");

    println!("\n📊 Metrics:");
    println!(
        "  Precision:        {:.2}% ({}/{})",
        metrics.precision * 100.0,
        metrics.true_positives,
        predicted.len()
    );
    println!(
        "  Recall:           {:.2}% ({}/{})",
        metrics.recall * 100.0,
        metrics.true_positives,
        expected.len()
    );
    println!("  F1 Score:         {:.2}%", metrics.f1_score * 100.0);

    if !predicted
        .intersection(expected)
        .collect::<Vec<_>>()
        .is_empty()
    {
        println!(
            "\n✓ Correctly Extracted Relations ({}):",
            metrics.true_positives
        );
        for triple in predicted.intersection(expected) {
            println!(
                "  {} → {} → {}",
                triple.subject, triple.predicate, triple.object
            );
        }
    }

    if !predicted
        .difference(expected)
        .collect::<Vec<_>>()
        .is_empty()
    {
        println!("\n✗ Incorrect Extractions ({}):", metrics.false_positives);
        for triple in predicted.difference(expected).take(5) {
            println!(
                "  {} → {} → {}",
                triple.subject, triple.predicate, triple.object
            );
        }
        if metrics.false_positives > 5 {
            println!("  ... and {} more", metrics.false_positives - 5);
        }
    }

    if !expected
        .difference(predicted)
        .collect::<Vec<_>>()
        .is_empty()
    {
        println!("\n✗ Missed Relations ({}):", metrics.false_negatives);
        for triple in expected.difference(predicted).take(5) {
            println!(
                "  {} → {} → {}",
                triple.subject, triple.predicate, triple.object
            );
        }
        if metrics.false_negatives > 5 {
            println!("  ... and {} more", metrics.false_negatives - 5);
        }
    }
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║     DocRED Document-Level Relation Extraction Evaluation     ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");

    // Load environment
    dotenvy::dotenv().ok();

    // Check configuration
    let has_api_key = env::var("GENAI_API_KEY").is_ok();
    let use_ollama = !has_api_key && is_ollama_available();

    if !has_api_key && !use_ollama {
        eprintln!("\n❌ No LLM available for document-level extraction!");
        eprintln!("\nDocument-level extraction requires strong reasoning capabilities.");
        eprintln!("\nRecommended setup:");
        eprintln!("1. Cloud LLM (best results):");
        eprintln!("   export GENAI_API_KEY=your-key");
        eprintln!("   export RDF_EXTRACTION_MODEL=claude-3-5-sonnet-20241022");
        eprintln!("\n2. Or use Ollama 70B (good results):");
        eprintln!("   ollama serve && ollama pull llama3.3:70b");
        eprintln!("   export RDF_EXTRACTION_MODEL=llama3.3:70b");
        std::process::exit(1);
    }

    // Configure extractor with higher context for documents
    let mut config = if use_ollama {
        println!("\n🦙 Using local Ollama (requires 70B model for good results)");
        env::set_var("GENAI_API_KEY", "ollama");
        if env::var("RDF_EXTRACTION_MODEL").is_err() {
            println!("⚠️  Warning: Using default model. For better results, use llama3.3:70b");
            env::set_var("RDF_EXTRACTION_MODEL", "llama3.3:70b");
        }
        ExtractionConfig::from_env()?
    } else {
        println!("\n☁️  Using cloud LLM");
        ExtractionConfig::from_env()?
    };

    // Document-level extraction benefits from more retries
    if config.max_retries < 5 {
        config.max_retries = 5;
    }

    println!("📝 Model: {}", config.model);
    println!("🔄 Max Retries: {}", config.max_retries);
    println!("📄 Document-level extraction enabled");

    // Load DocRED test cases
    let test_cases_path = "tests/fixtures/docred_sample.json";
    println!("\n📂 Loading DocRED samples from: {test_cases_path}");

    let contents = fs::read_to_string(test_cases_path).map_err(|e| {
        eprintln!("\n❌ Failed to load DocRED samples: {e}");
        eprintln!("Please create test fixtures. You can download samples from:");
        eprintln!("https://github.com/thunlp/DocRED");
        e
    })?;

    let documents: Vec<DocREDDocument> = serde_json::from_str(&contents)?;

    println!("✓ Loaded {} documents from DocRED dataset", documents.len());

    // Create extractor
    let extractor = GenAiExtractor::new(config)?;

    // Evaluate each document
    let mut all_metrics = Vec::new();

    for (_idx, doc) in documents.iter().enumerate().take(3) {
        // Limit to 3 for demo
        print_document_info(doc);

        // Get full document text
        let text = doc.get_full_text();

        println!(
            "\n📖 Extracting from {}-sentence document...",
            doc.sentence_count()
        );
        if std::env::var("DOCRED_SHOW_TEXT").is_ok() {
            println!("\nText:\n{text}\n");
        }

        // Extract RDF using document-level chunking pipeline
        // (automatically uses chunking for documents > 2000 tokens)
        let result = match extractor.extract_from_document(&text).await {
            Ok(doc) => {
                println!("✓ Extraction successful");
                doc
            }
            Err(e) => {
                println!("✗ Extraction failed: {e}");
                continue;
            }
        };

        // Extract triples
        let predicted_triples = extract_triples_from_jsonld(&result.data);

        // Apply heuristic filtering to reduce false positives
        let predicted_triples = filter_likely_incorrect_triples(predicted_triples);

        let expected_triples = docred_to_triples(doc);

        // Calculate metrics
        let metrics = DocumentMetrics::new(
            &predicted_triples,
            &expected_triples,
            doc.sentence_count(),
            doc.entities.len(),
        );

        all_metrics.push((metrics.precision, metrics.recall, metrics.f1_score));

        // Print report
        print_evaluation_report(doc, &predicted_triples, &expected_triples, &metrics);
    }

    // Print aggregate stats
    if !all_metrics.is_empty() {
        let avg_precision: f64 =
            all_metrics.iter().map(|(p, _, _)| p).sum::<f64>() / all_metrics.len() as f64;
        let avg_recall: f64 =
            all_metrics.iter().map(|(_, r, _)| r).sum::<f64>() / all_metrics.len() as f64;
        let avg_f1: f64 =
            all_metrics.iter().map(|(_, _, f)| f).sum::<f64>() / all_metrics.len() as f64;

        println!("\n\n═══════════════════════════════════════════════════════════════");
        println!("              📊 DOCUMENT-LEVEL AGGREGATE METRICS");
        println!("═══════════════════════════════════════════════════════════════");
        println!("\n📈 Average Performance:");
        println!("  Precision:    {:.2}%", avg_precision * 100.0);
        println!("  Recall:       {:.2}%", avg_recall * 100.0);
        println!("  F1 Score:     {:.2}%", avg_f1 * 100.0);

        println!("\n💡 Document-Level Challenges:");
        println!("  • Cross-sentence relation extraction");
        println!("  • Coreference resolution (\"he\", \"the company\", etc.)");
        println!("  • Long-range dependencies across paragraphs");
        println!("  • Entity disambiguation in context");

        println!("\n🎯 Expected Performance:");
        if avg_f1 >= 0.4 {
            println!("  Good! F1 ≥ 40% is respectable for document-level extraction");
        } else {
            println!("  Document-level extraction is challenging - F1 < 40%");
            println!("  Consider using a stronger model (claude-3-5-sonnet or gpt-4o)");
        }

        println!("\n═══════════════════════════════════════════════════════════════\n");
    }

    println!("✓ Document-level evaluation complete!\n");

    Ok(())
}
