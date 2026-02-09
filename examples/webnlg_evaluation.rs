//! `WebNLG` Evaluation Example
//!
//! This example demonstrates Back-Translation Testing using the `WebNLG` dataset:
//! 1. Extract RDF from natural language text
//! 2. Compare against gold standard triples
//! 3. Calculate precision, recall, and F1 scores
//! 4. Provide detailed comparison report
//!
//! ## `WebNLG` Dataset
//!
//! `WebNLG` is a manually curated dataset with 100% accurate triples (no "distant supervision" noise).
//! Contains 15 `DBpedia` categories including Building, Artist, Astronaut, Airport, etc.
//!
//! ## Running This Example
//!
//! With API key (uses cloud LLM):
//! ```bash
//! export GENAI_API_KEY="your-key"
//! cargo run --example webnlg_evaluation
//! ```
//!
//! With Ollama (free local LLM):
//! ```bash
//! ollama serve
//! ollama pull llama3.3:8b
//! cargo run --example webnlg_evaluation
//! ```

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;
use std::env;
use std::fs;
use text_to_rdf::normalize::normalize_predicate;
use text_to_rdf::{ExtractionConfig, GenAiExtractor, RdfExtractor};

/// A test case from the `WebNLG` dataset
#[derive(Debug, Deserialize)]
struct TestCase {
    id: String,
    raw_text: String,
    expected_triples: Vec<Triple>,
    #[allow(dead_code)]
    expected_jsonld: Value,
}

/// An RDF triple (subject-predicate-object)
#[derive(Debug, Deserialize, Serialize, Clone, PartialEq, Eq, Hash)]
struct Triple {
    subject: String,
    predicate: String,
    object: String,
}

/// Evaluation metrics for comparing predicted vs expected triples
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

/// Aggregate metrics across multiple test cases
#[derive(Debug)]
struct AggregateMetrics {
    total_cases: usize,
    avg_precision: f64,
    avg_recall: f64,
    avg_f1_score: f64,
    total_tp: usize,
    total_fp: usize,
    total_fn: usize,
}

impl AggregateMetrics {
    fn from_metrics(metrics: &[EvaluationMetrics]) -> Self {
        let total_cases = metrics.len();
        let avg_precision = metrics.iter().map(|m| m.precision).sum::<f64>() / total_cases as f64;
        let avg_recall = metrics.iter().map(|m| m.recall).sum::<f64>() / total_cases as f64;
        let avg_f1_score = metrics.iter().map(|m| m.f1_score).sum::<f64>() / total_cases as f64;
        let total_tp = metrics.iter().map(|m| m.true_positives).sum();
        let total_fp = metrics.iter().map(|m| m.false_positives).sum();
        let total_fn = metrics.iter().map(|m| m.false_negatives).sum();

        Self {
            total_cases,
            avg_precision,
            avg_recall,
            avg_f1_score,
            total_tp,
            total_fp,
            total_fn,
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
        for (key, value) in obj {
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
                    // Handle nested objects (like birthPlace, alumniOf, location)
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

/// Check if Ollama is available on localhost:11434
fn is_ollama_available() -> bool {
    use std::net::TcpStream;
    use std::time::Duration;

    // Check if Ollama is running
    if TcpStream::connect_timeout(
        &"127.0.0.1:11434".parse().unwrap(),
        Duration::from_millis(500),
    )
    .is_err()
    {
        return false;
    }

    // Check if llama3.3:8b model is available
    if let Ok(response) = ureq::get("http://127.0.0.1:11434/api/tags")
        .timeout(Duration::from_secs(2))
        .call()
    {
        if let Ok(body) = response.into_string() {
            return body.contains("llama3.3");
        }
    }

    false
}

/// Print a detailed comparison report for a single test case
fn print_test_case_report(
    test_case: &TestCase,
    predicted: &HashSet<Triple>,
    expected: &HashSet<Triple>,
    metrics: &EvaluationMetrics,
) {
    println!("\n═══════════════════════════════════════════════════════════════");
    println!("Test Case: {}", test_case.id);
    println!("═══════════════════════════════════════════════════════════════");
    println!("Input Text: \"{}\"", test_case.raw_text);

    println!("\n📊 Metrics:");
    println!("  Precision:        {:.2}% ({}/{})",
        metrics.precision * 100.0,
        metrics.true_positives,
        predicted.len()
    );
    println!("  Recall:           {:.2}% ({}/{})",
        metrics.recall * 100.0,
        metrics.true_positives,
        expected.len()
    );
    println!("  F1 Score:         {:.2}%", metrics.f1_score * 100.0);

    // True Positives
    let true_positives: Vec<_> = predicted.intersection(expected).collect();
    if !true_positives.is_empty() {
        println!("\n✓ True Positives ({}):", true_positives.len());
        for triple in true_positives {
            println!("  ({}, {}, {})", triple.subject, triple.predicate, triple.object);
        }
    }

    // False Positives
    let false_positives: Vec<_> = predicted.difference(expected).collect();
    if !false_positives.is_empty() {
        println!("\n✗ False Positives ({}) - Extracted but not in gold standard:", false_positives.len());
        for triple in false_positives {
            println!("  ({}, {}, {})", triple.subject, triple.predicate, triple.object);
        }
    }

    // False Negatives
    let false_negatives: Vec<_> = expected.difference(predicted).collect();
    if !false_negatives.is_empty() {
        println!("\n✗ False Negatives ({}) - In gold standard but not extracted:", false_negatives.len());
        for triple in false_negatives {
            println!("  ({}, {}, {})", triple.subject, triple.predicate, triple.object);
        }
    }
}

/// Print aggregate summary report
fn print_summary_report(aggregate: &AggregateMetrics) {
    println!("\n\n");
    println!("═══════════════════════════════════════════════════════════════");
    println!("                  📊 AGGREGATE SUMMARY REPORT");
    println!("═══════════════════════════════════════════════════════════════");

    println!("\n📈 Overall Performance:");
    println!("  Total Test Cases:    {}", aggregate.total_cases);
    println!("  Average Precision:   {:.2}%", aggregate.avg_precision * 100.0);
    println!("  Average Recall:      {:.2}%", aggregate.avg_recall * 100.0);
    println!("  Average F1 Score:    {:.2}%", aggregate.avg_f1_score * 100.0);

    println!("\n🎯 Triple Statistics:");
    println!("  True Positives:      {}", aggregate.total_tp);
    println!("  False Positives:     {}", aggregate.total_fp);
    println!("  False Negatives:     {}", aggregate.total_fn);

    let total_predicted = aggregate.total_tp + aggregate.total_fp;
    let total_expected = aggregate.total_tp + aggregate.total_fn;
    println!("  Total Predicted:     {total_predicted}");
    println!("  Total Expected:      {total_expected}");

    // Quality assessment
    println!("\n🏆 Quality Assessment:");
    if aggregate.avg_f1_score >= 0.9 {
        println!("  Excellent! F1 ≥ 90% - Production-ready quality");
    } else if aggregate.avg_f1_score >= 0.75 {
        println!("  Good! F1 ≥ 75% - Acceptable for most use cases");
    } else if aggregate.avg_f1_score >= 0.6 {
        println!("  Fair. F1 ≥ 60% - May need prompt tuning or better model");
    } else {
        println!("  Needs Improvement. F1 < 60% - Consider prompt engineering or model selection");
    }

    println!("\n═══════════════════════════════════════════════════════════════\n");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║       WebNLG Back-Translation Testing & Evaluation           ║");
    println!("╚═══════════════════════════════════════════════════════════════╝");

    // Load .env file if it exists
    dotenvy::dotenv().ok();

    // Check configuration
    let has_api_key = env::var("GENAI_API_KEY").is_ok();
    let use_ollama = !has_api_key && is_ollama_available();

    if !has_api_key && !use_ollama {
        eprintln!("\n❌ No LLM available!");
        eprintln!("\nTo run this example:");
        eprintln!("1. Set GENAI_API_KEY environment variable, OR");
        eprintln!("2. Start Ollama:");
        eprintln!("   ollama serve");
        eprintln!("   ollama pull llama3.3:8b");
        std::process::exit(1);
    }

    // Configure extractor
    let config = if use_ollama {
        println!("\n🦙 Using local Ollama (llama3.3:8b)");
        env::set_var("GENAI_API_KEY", "ollama");
        env::set_var("RDF_EXTRACTION_MODEL", "llama3.3:8b");
        ExtractionConfig::from_env()?
    } else {
        println!("\n☁️  Using cloud LLM");
        ExtractionConfig::from_env()?
    };

    println!("📝 Model: {}", config.model);
    println!("🔄 Max Retries: {}", config.max_retries);
    println!("✓ Strict Validation: {}", config.strict_validation);

    // Load WebNLG test cases
    let test_cases_path = "tests/fixtures/test_cases.json";
    println!("\n📂 Loading test cases from: {test_cases_path}");

    let contents = fs::read_to_string(test_cases_path)?;
    let test_cases: Vec<TestCase> = serde_json::from_str(&contents)?;

    println!("✓ Loaded {} test cases from WebNLG dataset", test_cases.len());

    // Create extractor
    let extractor = GenAiExtractor::new(config)?;

    // Run evaluation on all test cases
    let mut all_metrics = Vec::new();

    for (idx, test_case) in test_cases.iter().enumerate() {
        println!("\n\n[{}/{}] Processing: {}", idx + 1, test_cases.len(), test_case.id);
        println!("Input: \"{}\"", test_case.raw_text);

        // Extract RDF from text
        print!("Extracting... ");
        let result = match extractor.extract(&test_case.raw_text).await {
            Ok(doc) => {
                println!("✓");
                doc
            }
            Err(e) => {
                println!("✗ Error: {e}");
                continue;
            }
        };

        // Debug: Show what was extracted
        if std::env::var("WEBNLG_DEBUG").is_ok() {
            println!("\n🔍 Debug - Extracted JSON-LD:");
            println!("{}", serde_json::to_string_pretty(&result.data).unwrap_or_default());
        }

        // Extract triples from predicted and expected JSON-LD
        let predicted_triples = extract_triples_from_jsonld(&result.data);
        let expected_triples: HashSet<Triple> = test_case.expected_triples.iter().cloned().collect();

        // Calculate metrics
        let metrics = EvaluationMetrics::new(&predicted_triples, &expected_triples);

        // Print quick summary
        println!("F1: {:.2}% | P: {:.2}% | R: {:.2}%",
            metrics.f1_score * 100.0,
            metrics.precision * 100.0,
            metrics.recall * 100.0
        );

        // Store for aggregate report
        all_metrics.push(EvaluationMetrics {
            precision: metrics.precision,
            recall: metrics.recall,
            f1_score: metrics.f1_score,
            true_positives: metrics.true_positives,
            false_positives: metrics.false_positives,
            false_negatives: metrics.false_negatives,
        });

        // Print detailed report for this test case
        print_test_case_report(test_case, &predicted_triples, &expected_triples, &metrics);
    }

    // Print aggregate summary
    if !all_metrics.is_empty() {
        let aggregate = AggregateMetrics::from_metrics(&all_metrics);
        print_summary_report(&aggregate);
    }

    println!("✓ Evaluation complete!\n");

    Ok(())
}
