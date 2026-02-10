//! Test entity consistency across related documents
//!
//! This example demonstrates:
//! 1. Processing two related Wikipedia articles (Marie & Pierre Curie)
//! 2. Verifying that shared entities are mapped consistently
//! 3. Comparing RDF representations across documents
//!
//! Run:
//! ```bash
//! export GENAI_API_KEY=ollama
//! export RDF_EXTRACTION_MODEL=qwen2.5:7b
//! cargo run --example test_entity_consistency
//! ```

use std::fs;
use std::time::Instant;
use text_to_rdf::{ExtractionConfig, GenAiExtractor, RdfDocument};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║   Entity Consistency Test: Marie & Pierre Curie Articles    ║");
    println!("╚═══════════════════════════════════════════════════════════════╝\n");

    // Load both Wikipedia articles
    let marie_text = fs::read_to_string("tests/fixtures/wikipedia_marie_curie.txt")?;
    let pierre_text = fs::read_to_string("tests/fixtures/wikipedia_pierre_curie.txt")?;

    println!("📄 Documents:");
    println!(
        "   1. Marie Curie:  {} chars (~{} tokens)",
        marie_text.len(),
        marie_text.len() / 4
    );
    println!(
        "   2. Pierre Curie: {} chars (~{} tokens)\n",
        pierre_text.len(),
        pierre_text.len() / 4
    );

    // Create extractor
    let config = ExtractionConfig::from_env()?;
    let extractor = GenAiExtractor::new(config)?;

    println!("🔄 Processing Marie Curie article...");
    println!("{}", "─".repeat(65));
    let start = Instant::now();
    let marie_result = extractor.extract_from_document(&marie_text).await?;
    let marie_time = start.elapsed();
    println!(
        "✅ Marie Curie extracted in {:.2}s\n",
        marie_time.as_secs_f64()
    );

    println!("🔄 Processing Pierre Curie article...");
    println!("{}", "─".repeat(65));
    let start = Instant::now();
    let pierre_result = extractor.extract_from_document(&pierre_text).await?;
    let pierre_time = start.elapsed();
    println!(
        "✅ Pierre Curie extracted in {:.2}s\n",
        pierre_time.as_secs_f64()
    );

    // Save results
    fs::create_dir_all("output")?;

    let marie_json = serde_json::to_string_pretty(&marie_result)?;
    fs::write("output/marie_curie_extraction.json", &marie_json)?;

    let pierre_json = serde_json::to_string_pretty(&pierre_result)?;
    fs::write("output/pierre_curie_extraction.json", &pierre_json)?;

    // Analyze results
    println!("{}", "=".repeat(65));
    println!("📊 Extraction Results:\n");

    println!("Marie Curie Document:");
    print_document_stats(&marie_result, &marie_json);

    println!("\nPierre Curie Document:");
    print_document_stats(&pierre_result, &pierre_json);

    // Find shared entities
    println!("\n{}", "=".repeat(65));
    println!("🔍 Entity Consistency Analysis:\n");

    let marie_entities = extract_entity_names(&marie_result.data);
    let pierre_entities = extract_entity_names(&pierre_result.data);

    println!("Entities in Marie Curie article: {}", marie_entities.len());
    println!(
        "Entities in Pierre Curie article: {}",
        pierre_entities.len()
    );

    // Find shared entities
    let mut shared = Vec::new();
    for entity in &marie_entities {
        if pierre_entities.contains(entity) {
            shared.push(entity);
        }
    }

    println!("\n🔗 Shared entities found in both articles:");
    if shared.is_empty() {
        println!("   (None detected - may indicate entities referenced differently)");
    } else {
        for entity in &shared {
            println!("   - {}", entity);
        }
    }

    // Look for cross-references
    println!("\n🔎 Cross-references:");
    if marie_text.to_lowercase().contains("pierre") {
        println!("   ✅ Marie article mentions Pierre");
    }
    if pierre_text.to_lowercase().contains("marie") {
        println!("   ✅ Pierre article mentions Marie");
    }

    // Show key entities from each
    println!("\n📋 Top entities from each document:");
    println!("\nMarie Curie article:");
    for (i, entity) in marie_entities.iter().take(5).enumerate() {
        println!("   {}. {}", i + 1, entity);
    }

    println!("\nPierre Curie article:");
    for (i, entity) in pierre_entities.iter().take(5).enumerate() {
        println!("   {}. {}", i + 1, entity);
    }

    println!("\n{}", "=".repeat(65));
    println!("💾 Full outputs saved:");
    println!("   - output/marie_curie_extraction.json");
    println!("   - output/pierre_curie_extraction.json");
    println!("\n📖 To view: cat output/marie_curie_extraction.json");
    println!("📖 To compare: diff -u output/marie_curie_extraction.json output/pierre_curie_extraction.json");

    Ok(())
}

fn print_document_stats(doc: &RdfDocument, json: &str) {
    let entity_count = count_json_objects(&doc.data, "@type");
    let array_count = count_json_arrays(&doc.data);

    println!("   Size: {} characters", json.len());
    println!("   Entities: ~{}", entity_count);
    println!("   Relations/Arrays: ~{}", array_count);
}

fn extract_entity_names(value: &serde_json::Value) -> Vec<String> {
    let mut names = Vec::new();
    extract_names_recursive(value, &mut names);
    names.sort();
    names.dedup();
    names
}

fn extract_names_recursive(value: &serde_json::Value, names: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            // Look for name fields
            if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                names.push(name.to_string());
            }

            // Recurse into all values
            for v in map.values() {
                extract_names_recursive(v, names);
            }
        }
        serde_json::Value::Array(arr) => {
            for v in arr {
                extract_names_recursive(v, names);
            }
        }
        _ => {}
    }
}

fn count_json_objects(value: &serde_json::Value, field: &str) -> usize {
    match value {
        serde_json::Value::Object(map) => {
            let mut count = if map.contains_key(field) { 1 } else { 0 };
            for v in map.values() {
                count += count_json_objects(v, field);
            }
            count
        }
        serde_json::Value::Array(arr) => arr.iter().map(|v| count_json_objects(v, field)).sum(),
        _ => 0,
    }
}

fn count_json_arrays(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => map.values().map(|v| count_json_arrays(v)).sum(),
        serde_json::Value::Array(arr) => {
            1 + arr.iter().map(|v| count_json_arrays(v)).sum::<usize>()
        }
        _ => 0,
    }
}
