//! Example: Entity Linking and Validation Pipeline
//!
//! Demonstrates the multi-stage RDF extraction pipeline with:
//! - Stage 1: Text extraction (LLM)
//! - Stage 2: Entity linking (DBpedia)
//! - Stage 5: SHACL-like validation

use text_to_rdf::{
    EntityLinker, ExtractionConfig, GenAiExtractor,
    RdfExtractor, RdfValidator,
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== Multi-Stage RDF Extraction Pipeline ===\n");

    // Stage 1: Load configuration and create extractor
    let config = ExtractionConfig::from_env()?;
    let extractor = GenAiExtractor::new(config.clone())?;

    let text = "Alan Bean was born on March 15, 1932 in Texas. He was an American astronaut.";

    println!("Input Text: {}\n", text);

    // Stage 2: Extract RDF with LLM
    println!("Stage 1: LLM Extraction");
    println!("======================");
    let mut result = extractor.extract(text).await?;
    println!("Extracted JSON-LD:");
    println!("{}\n", result.to_json()?);

    // Stage 3: Entity Linking (if enabled)
    println!("Stage 2: Entity Linking");
    println!("=======================");
    if config.entity_linker.enabled {
        let linker = EntityLinker::new(config.entity_linker)?;

        if let Some(name) = result.get_name() {
            println!("Linking entity: {}", name);

            match linker.link_entity(text, name, result.get_type()).await {
                Ok(Some(linked)) => {
                    println!("✓ Linked to: {}", linked.uri);
                    println!("  Confidence: {:.2}", linked.confidence);
                    println!("  Types: {:?}", linked.types);

                    // Enrich document with canonical URI
                    result.enrich_with_uri(linked.uri);
                }
                Ok(None) => {
                    println!("✗ No entity link found (confidence too low or service unavailable)");
                }
                Err(e) => {
                    println!("✗ Entity linking failed: {}", e);
                }
            }
        }
    } else {
        println!("Entity linking disabled (set ENTITY_LINKING_ENABLED=true to enable)");
    }
    println!();

    // Stage 5: Validation
    println!("Stage 5: Validation");
    println!("===================");
    let validator = RdfValidator::with_schema_org_rules();
    let validation = validator.validate(&result);

    if validation.is_valid() {
        println!("✓ Validation PASSED");
    } else {
        println!("✗ Validation FAILED");
    }

    if !validation.violations.is_empty() {
        println!("\nViolations:");
        for violation in &validation.violations {
            println!(
                "  [{:?}] {}: {}",
                violation.severity, violation.rule, violation.message
            );
        }
    } else {
        println!("  No violations found");
    }
    println!();

    // Final output
    println!("Final JSON-LD:");
    println!("==============");
    println!("{}", result.to_json()?);

    Ok(())
}
