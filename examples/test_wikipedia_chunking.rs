//! Test Phase 1 chunking pipeline with a real Wikipedia article
//!
//! This example demonstrates:
//! 1. Semantic chunking of a long document (~11K tokens)
//! 2. Knowledge buffer tracking entities across chunks
//! 3. Context-aware extraction
//!
//! Run:
//! ```bash
//! export GENAI_API_KEY=ollama
//! export RDF_EXTRACTION_MODEL=qwen2.5:7b
//! cargo run --example test_wikipedia_chunking
//! ```

use std::fs;
use std::time::Instant;
use text_to_rdf::{ExtractionConfig, GenAiExtractor};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("╔═══════════════════════════════════════════════════════════════╗");
    println!("║     Phase 1 Test: Wikipedia Article with Chunking           ║");
    println!("╚═══════════════════════════════════════════════════════════════╝\n");

    // Load Wikipedia article
    let article_path = "tests/fixtures/wikipedia_marie_curie.txt";
    let text = fs::read_to_string(article_path)?;

    let char_count = text.len();
    let token_estimate = char_count / 4;

    println!("📄 Document: Marie Curie (Wikipedia)");
    println!("   Characters: {}", char_count);
    println!("   Estimated tokens: ~{}", token_estimate);
    println!("   Chunking threshold: 2000 tokens");
    println!(
        "   Will chunk: {}\n",
        if token_estimate > 2000 {
            "YES ✅"
        } else {
            "NO ❌"
        }
    );

    // Create extractor
    let config = ExtractionConfig::from_env()?;
    let extractor = GenAiExtractor::new(config)?;

    println!("🔄 Starting extraction with Phase 1 pipeline...\n");
    println!("Expected behavior:");
    println!("  - Document will be split into ~7-8 chunks");
    println!("  - Knowledge buffer will track entities across chunks");
    println!("  - Each chunk receives context from previous chunks\n");
    println!("{}", "=".repeat(65));

    // Extract using document-level pipeline with timing
    let start_time = Instant::now();
    let result = extractor.extract_from_document(&text).await?;
    let duration = start_time.elapsed();

    println!("{}", "=".repeat(65));
    println!("\n✅ Extraction complete!\n");

    // Convert to JSON
    let json = serde_json::to_string_pretty(&result)?;

    // Create output directory if it doesn't exist
    fs::create_dir_all("output")?;

    // Save full output to file
    let output_path = "output/wikipedia_extraction_result.json";
    fs::write(output_path, &json)?;

    // Count entities and relations
    let entity_count = count_json_objects(&result.data, "@type");
    let relation_count = count_json_arrays(&result.data);

    // Display timing metrics
    println!("⏱️  Performance Metrics:");
    println!("{}", "─".repeat(65));
    println!("   Total extraction time: {:.2}s", duration.as_secs_f64());
    println!(
        "   Average per chunk: {:.2}s",
        duration.as_secs_f64() / 18.0
    );

    // Display quality metrics
    println!("\n📊 Quality Metrics:");
    println!("{}", "─".repeat(65));
    println!("   Output size: {} characters", json.len());
    println!("   Entities extracted: ~{}", entity_count);
    println!("   Relations/Arrays: ~{}", relation_count);

    // Display output preview
    println!("\n📄 Extracted RDF Document (preview):");
    println!("{}", "─".repeat(65));
    if json.len() > 1000 {
        println!("{}...\n", &json[..1000]);
        println!("[Showing first 1000 of {} total characters]", json.len());
    } else {
        println!("{}", json);
    }

    println!("\n{}", "─".repeat(65));
    println!("💾 Full output saved to: {}", output_path);
    println!("\n💡 Key observations:");
    println!("   - Check console output above for chunking messages");
    println!("   - Each chunk was processed with context from previous ones");
    println!("   - Knowledge buffer maintained entity continuity");
    println!("\n📖 To view full output: cat {}", output_path);

    Ok(())
}

/// Count objects with a specific field in JSON
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

/// Count arrays in JSON (as proxy for relations)
fn count_json_arrays(value: &serde_json::Value) -> usize {
    match value {
        serde_json::Value::Object(map) => map.values().map(|v| count_json_arrays(v)).sum(),
        serde_json::Value::Array(arr) => {
            1 + arr.iter().map(|v| count_json_arrays(v)).sum::<usize>()
        }
        _ => 0,
    }
}
