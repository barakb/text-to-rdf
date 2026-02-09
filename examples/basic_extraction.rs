//! Basic example of using text-to-rdf with .env configuration
//!
//! Before running this example:
//! 1. Copy .env.example to .env
//! 2. Add your GENAI_API_KEY to .env
//! 3. Run: cargo run --example basic_extraction

use text_to_rdf::{ExtractionConfig, GenAiExtractor, RdfExtractor};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration from .env file
    let config = ExtractionConfig::from_env()?;

    println!("Using model: {}", config.model);
    println!("Temperature: {:?}", config.temperature);
    println!();

    // Create the extractor
    let extractor = GenAiExtractor::new(config)?;

    // Example texts to extract from
    let examples = vec![
        "Alan Bean was born on the 15th of March 1932.",
        "MIT was founded in 1861 in Cambridge, Massachusetts.",
        "Albert Einstein won the Nobel Prize in Physics in 1921.",
    ];

    for (i, text) in examples.iter().enumerate() {
        println!("--- Example {} ---", i + 1);
        println!("Input: {}", text);
        println!();

        // Extract RDF entities
        match extractor.extract(text).await {
            Ok(result) => {
                println!("Extracted JSON-LD:");
                println!("{}", result.to_json()?);
                println!();

                if let Some(entity_type) = result.get_type() {
                    println!("Entity Type: {}", entity_type);
                }
                if let Some(entity_id) = result.get_id() {
                    println!("Entity ID: {}", entity_id);
                }
            }
            Err(e) => {
                eprintln!("Error extracting RDF: {}", e);
            }
        }

        println!();
        println!("---");
        println!();
    }

    Ok(())
}
