# Entity Linking Demo

This example demonstrates entity linking with the text-to-rdf library.

## What is Entity Linking?

Entity linking resolves entity names (like "Alan Bean") to canonical URIs in knowledge bases like DBpedia or Wikidata. This prevents duplicate entities in your knowledge graph and links to authoritative sources.

**Without entity linking:**
```json
{
  "@id": "https://example.org/person/alan-bean",
  "name": "Alan Bean"
}
```

**With entity linking:**
```json
{
  "@id": "http://dbpedia.org/resource/Alan_Bean",
  "name": "Alan Bean"
}
```

## Configuration

Entity linking is configured via environment variables:

```bash
# Enable entity linking
ENTITY_LINKING_ENABLED=true

# Choose strategy: local, dbpedia, wikidata, or none
ENTITY_LINKING_STRATEGY=local

# For local strategy (Rust-native, no network required):
ENTITY_LINKING_KB_PATH=/path/to/your/knowledge-base

# For DBpedia Spotlight (remote API):
# ENTITY_LINKING_STRATEGY=dbpedia
# ENTITY_LINKING_SERVICE_URL=https://api.dbpedia-spotlight.org/en

# Confidence threshold (0.0-1.0)
ENTITY_LINKING_CONFIDENCE=0.5
```

## Strategies

### Local Strategy (Recommended for Production)

The **Local** strategy uses Oxigraph, a Rust-native RDF database, to query a local knowledge base via SPARQL. This approach:

- ✅ **No network dependency**: All data is local
- ✅ **Fast**: Direct SPARQL queries against local store
- ✅ **Privacy-friendly**: No external API calls
- ✅ **Production-ready**: Embedded in your binary
- ✅ **Supports Wikidata & DBpedia**: Load any RDF dataset

**Setup:**

1. Download or create a local RDF knowledge base (e.g., mini-wikidata dump)
2. Set `ENTITY_LINKING_STRATEGY=local`
3. Set `ENTITY_LINKING_KB_PATH=/path/to/kb`

The local linker queries for entities using SPARQL:
```sparql
SELECT ?entity ?label ?type WHERE {
  { ?entity rdfs:label ?label . }
  UNION
  { ?entity schema:name ?label . }
  OPTIONAL { ?entity rdf:type ?type }
}
```

### DBpedia Spotlight (Remote API)

DBpedia Spotlight is a web service for entity linking. Good for prototyping but has limitations:

- ⚠️ Requires network access
- ⚠️ Public API may have rate limits
- ⚠️ Availability issues

### Wikidata (Coming Soon)

Direct Wikidata API integration (not yet implemented).

## Running the Example

```bash
# Basic extraction without entity linking
ENTITY_LINKING_ENABLED=false cargo run --example pipeline_demo

# With DBpedia entity linking (requires network access)
ENTITY_LINKING_ENABLED=true cargo run --example pipeline_demo
```

## Note on DBpedia Spotlight

The public DBpedia Spotlight API (`https://api.dbpedia-spotlight.org`) may have availability issues or rate limits. For production use, consider:

1. **Self-hosting DBpedia Spotlight**: https://github.com/dbpedia-spotlight/dbpedia-spotlight-model
2. **Using Wikidata API**: Set `ENTITY_LINKING_STRATEGY=wikidata` (implementation coming soon)
3. **Running your own instance**: Docker images available for DBpedia Spotlight

## Testing

Run the entity linking integration test:

```bash
# This test requires network access and a working DBpedia Spotlight endpoint
cargo test test_entity_linking_integration -- --ignored --nocapture
```

Run validation tests with URIs:

```bash
# Tests validation of documents with canonical URIs
cargo test test_validation_with_linking
```

## Example Output

When entity linking succeeds:

```
Stage 2: Entity Linking
=======================
Linking entity: alan_bean
✓ Linked to: http://dbpedia.org/resource/Alan_Bean
  Confidence: 0.98
  Types: ["Person", "Astronaut"]

Final JSON-LD:
{
  "@context": "https://schema.org/",
  "@type": "Person",
  "@id": "http://dbpedia.org/resource/Alan_Bean",
  "name": "Alan Bean",
  "birthDate": "1932-03-15"
}
```

## Code Example

**Using Local Strategy:**

```rust
use text_to_rdf::{
    EntityLinker, EntityLinkerConfig, LinkingStrategy,
    ExtractionConfig, GenAiExtractor, RdfExtractor
};
use std::path::PathBuf;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure local entity linking
    let mut config = ExtractionConfig::from_env()?;
    config.entity_linker.enabled = true;
    config.entity_linker.strategy = LinkingStrategy::Local;
    config.entity_linker.local_kb_path = Some(PathBuf::from("/path/to/kb"));

    // Extract RDF
    let extractor = GenAiExtractor::new(config.clone())?;
    let mut doc = extractor.extract("Alan Bean was an astronaut").await?;

    // Link entities using local KB
    let linker = EntityLinker::new(config.entity_linker)?;
    if let Some(name) = doc.get_name() {
        if let Ok(Some(entity)) = linker.link_entity(
            "Alan Bean was an astronaut",
            name,
            doc.get_type()
        ).await {
            // Enrich with canonical URI from local KB
            doc.enrich_with_uri(entity.uri);
            println!("Linked to: {}", doc.get_id().unwrap());
        }
    }

    Ok(())
}
```

**Using DBpedia Spotlight:**

```rust
use text_to_rdf::{
    EntityLinker, EntityLinkerConfig, LinkingStrategy,
    ExtractionConfig, GenAiExtractor, RdfExtractor
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Configure entity linking
    let mut config = ExtractionConfig::from_env()?;
    config.entity_linker.enabled = true;
    config.entity_linker.strategy = LinkingStrategy::DbpediaSpotlight;

    // Extract RDF
    let extractor = GenAiExtractor::new(config.clone())?;
    let mut doc = extractor.extract("Alan Bean was an astronaut").await?;

    // Link entities
    let linker = EntityLinker::new(config.entity_linker)?;
    if let Some(name) = doc.get_name() {
        if let Ok(Some(entity)) = linker.link_entity(
            "Alan Bean was an astronaut",
            name,
            doc.get_type()
        ).await {
            // Enrich with canonical URI
            doc.enrich_with_uri(entity.uri);
            println!("Linked to: {}", doc.get_id().unwrap());
        }
    }

    Ok(())
}
```

## Benefits

1. **Eliminates duplicates**: "Alan Bean", "A. Bean", "Bean, Alan" all map to the same URI
2. **Rich metadata**: Get types, alternative names, and related entities from DBpedia
3. **Graph integration**: Easy to integrate with existing knowledge graphs
4. **Standards compliance**: Follows Linked Data best practices
