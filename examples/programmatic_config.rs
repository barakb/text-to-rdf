//! Example showing programmatic configuration without .env file
//!
//! This example demonstrates how to configure the library
//! using the builder pattern instead of environment variables.
//!
//! Note: You still need GENAI_API_KEY in your environment for the genai crate.
//! Run: GENAI_API_KEY=your-key cargo run --example programmatic_config

use text_to_rdf::{ExtractionConfig, GenAiExtractor, RdfExtractor};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure using builder pattern
    let config = ExtractionConfig::new()
        .with_model("claude-3-5-sonnet")
        .with_temperature(0.2) // Lower temperature for more consistent results
        .with_max_tokens(2048)
        .with_ontology("https://schema.org/")
        .with_ontology("http://www.w3.org/2006/time#"); // Add additional ontology

    println!("Configuration:");
    println!("  Model: {}", config.model);
    println!("  Temperature: {:?}", config.temperature);
    println!("  Max Tokens: {:?}", config.max_tokens);
    println!("  Ontologies: {:?}", config.ontologies);
    println!();

    let extractor = GenAiExtractor::new(config)?;

    let text = "The Apollo 11 mission launched on July 16, 1969, from Kennedy Space Center.";

    println!("Input: {}", text);
    println!();

    let result = extractor.extract_and_validate(text).await?;

    println!("Extracted and validated RDF:");
    println!("{}", result.to_json()?);

    Ok(())
}
