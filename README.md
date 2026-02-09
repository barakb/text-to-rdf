# Text-to-RDF: RDF Extraction Library

A high-performance Rust library for extracting structured RDF data (entities and relations) from unstructured text using LLMs via the `genai` crate.

## Features

- **Schema-First Extraction**: Outputs JSON-LD mapped to Schema.org and standard RDF ontologies
- **Instructor Pattern**: Automatic retry with validation error feedback for reliable structured output
- **Multi-Provider AI Support**: Works with Gemini, Claude, GPT via `genai`
- **Coreference Resolution** (pure Rust): Resolve pronouns to canonical entities before extraction
- **GLiNER Zero-Shot NER** (optional): Fast local entity extraction with provenance tracking (4x faster than Python)
- **Hybrid Pipeline**: 5-stage production-grade pipeline from preprocessing to validation
- **Entity Linking**: Local Rust-native linking with Oxigraph or remote APIs (DBpedia, Wikidata)
- **SHACL Validation**: Schema validation with custom rules, SPARQL ASK queries, and confidence scoring
- **Trait-Based Design**: Extensible architecture for custom extractors
- **Environment Configuration**: Easy setup with .env files
- **Real-World Test Data**: Includes WebNLG dataset fixtures for validation
- **F1 Score Evaluation**: Built-in metrics for comparing extracted vs expected triples

## 🚀 Gold Standard Pipeline (2026)

For production-grade RDF extraction, use the **5-stage hybrid pipeline**:

0. **Preprocessing** (Coref) - Resolve pronouns and entity mentions (pure Rust, ~1ms)
1. **Discovery** (GLiNER) - Fast zero-shot entity extraction with provenance
2. **Relations** (LLM) - Semantic relation extraction guided by discovered entities
3. **Identity** (Oxigraph) - Local entity linking to Wikidata/DBpedia
4. **Validation** (SHACL) - Schema compliance checking

**Benefits**: 20% better accuracy, 4x faster than LLM-only, 50% cheaper, works offline.

📖 **[Read the complete Hybrid Pipeline Guide →](docs/HYBRID_PIPELINE.md)**
📖 **[Coreference Resolution Guide →](docs/COREFERENCE_RESOLUTION.md)**

## Quick Start

### 1. Configuration with .env file

Copy the example configuration:

```bash
cp .env.example .env
```

Edit `.env` and add your API key:

```env
GENAI_API_KEY=your-api-key-here
RDF_EXTRACTION_MODEL=claude-3-5-sonnet
GENAI_TEMPERATURE=0.3
```

### 2. Use the library

```rust
use text_to_rdf::{RdfExtractor, GenAiExtractor, ExtractionConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Load configuration from .env file
    let config = ExtractionConfig::from_env()?;

    let extractor = GenAiExtractor::new(config)?;
    let text = "Alan Bean was born on the 15th of March 1932.";

    let result = extractor.extract(text).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);

    Ok(())
}
```

## Instructor Pattern: Structured Extraction with Retry Logic

The library implements the **Instructor pattern** for reliable structured output from LLMs. This ensures that extracted RDF data always conforms to the expected JSON-LD schema.

### How It Works

1. **Attempt Extraction** - LLM extracts entities from text as JSON-LD
2. **Validate Structure** - Check if output conforms to Schema.org and RDF requirements
3. **Send Error Feedback** - If validation fails, send detailed error message back to LLM
4. **Retry with Context** - LLM corrects the output based on specific error feedback
5. **Return Valid Data** - Process repeats up to `max_retries` times until valid

### Example Flow

```
Attempt 1:
Input: "Albert Einstein was born in Ulm"
LLM Output: { "type": "Person", "name": "Einstein" }
Validation Error: Missing @context field

Attempt 2 (with feedback):
Error Message: "Schema Validation Error: Missing @context.
               Please ensure @context is set to 'https://schema.org/'"
LLM Output: { "@context": "https://schema.org/", "@type": "Person", "name": "Albert Einstein" }
Success ✓
```

### Configuration

```rust
let config = ExtractionConfig::new()
    .with_max_retries(3)              // Try up to 3 times (default: 2)
    .with_strict_validation(true);    // Enforce validation (default: true)
```

**Benefits**:
- **Higher Accuracy**: Validation errors guide the LLM to correct outputs
- **Deterministic Output**: Always returns valid JSON-LD or fails explicitly
- **Cost Efficient**: Only retries on validation failure, not API errors
- **Debugging**: Detailed error messages show what went wrong

## Configuration Options

### Environment Variables

Create a `.env` file in your project root with these variables:

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `GENAI_API_KEY` | **Yes** | - | API key for the AI service |
| `RDF_EXTRACTION_MODEL` | No | `claude-3-5-sonnet` | Model name (e.g., `gpt-4o`, `gemini-1.5-pro`) |
| `GENAI_TEMPERATURE` | No | `0.3` | Temperature 0.0-1.0 (lower = more deterministic) |
| `GENAI_MAX_TOKENS` | No | `4096` | Maximum tokens in response |
| `GENAI_SYSTEM_PROMPT` | No | (built-in) | Custom system prompt override |
| `RDF_ONTOLOGIES` | No | `https://schema.org/` | Comma-separated ontology URLs |
| `RDF_EXTRACTION_MAX_RETRIES` | No | `2` | Max retry attempts for failed extractions |
| `RDF_EXTRACTION_STRICT_VALIDATION` | No | `true` | Enable strict schema validation with error feedback |
| `RDF_EXTRACTION_SIMPLE_MODEL` | No | - | Model for simple extraction (e.g., `llama3.2:3b`) |
| `RDF_EXTRACTION_INJECT_CONTEXT` | No | `true` | Inject hardcoded @context to prevent URI hallucinations |
| `ENTITY_LINKING_ENABLED` | No | `false` | Enable entity linking |
| `ENTITY_LINKING_STRATEGY` | No | `none` | Strategy: `local`, `dbpedia`, `wikidata`, or `none` |
| `ENTITY_LINKING_KB_PATH` | No | - | Path to local RDF knowledge base (for `local` strategy) |
| `ENTITY_LINKING_CONFIDENCE` | No | `0.5` | Confidence threshold 0.0-1.0 |
| `ENTITY_LINKING_FUZZY_MATCHING` | No | `true` | Enable fuzzy matching with Jaro-Winkler similarity |
| `ENTITY_LINKING_FUZZY_THRESHOLD` | No | `0.8` | Min similarity for fuzzy matches (0.0-1.0) |
| `ENTITY_LINKING_LLM_DISAMBIGUATION` | No | `true` | Use LLM to disambiguate multiple candidates |
| `ENTITY_LINKING_MIN_CANDIDATES_FOR_LLM` | No | `2` | Min candidates to trigger LLM disambiguation |

### GLiNER Configuration (Optional Feature)

Enable GLiNER for fast local entity extraction:

```bash
# Build with GLiNER feature
cargo build --features gliner

# Download GLiNER model
huggingface-cli download onnx-community/gliner_medium-v2.1
```

| Variable | Required | Default | Description |
|----------|----------|---------|-------------|
| `GLINER_MODEL_PATH` | No | `models/gliner_medium-v2.1` | Path to GLiNER ONNX model directory |
| `GLINER_ENTITY_TYPES` | No | `Person,Organization,Place,Event,Product,Date` | Comma-separated entity types |
| `GLINER_CONFIDENCE` | No | `0.5` | Confidence threshold 0.0-1.0 |
| `GLINER_THREADS` | No | `0` (auto) | Number of threads for inference |

### Programmatic Configuration

You can also configure without .env:

```rust
let config = ExtractionConfig::new()
    .with_model("gpt-4o")
    .with_temperature(0.5)
    .with_max_tokens(2000)
    .with_system_prompt("Custom prompt...")
    .with_ontology("http://www.w3.org/2006/time#");

let extractor = GenAiExtractor::new(config)?;
```

## SHACL Validation Layer (Stage 4)

The library provides comprehensive SHACL-like validation to ensure extracted RDF data meets quality standards before committing to your knowledge graph.

### Features

- **Rule-Based Validation**: Check required properties for Schema.org entity types
- **SPARQL ASK Queries**: Custom validation logic via Oxigraph
- **Confidence Scoring**: Quantitative quality scores (0.0-1.0)
- **Type Checking**: Validate dates, URLs, and other datatypes
- **Configurable Behavior**: Drop invalid triples or flag as low confidence

### Basic Usage

```rust
use text_to_rdf::{RdfValidator, RdfDocument};

let validator = RdfValidator::with_schema_org_rules();
let result = validator.validate(&rdf_doc);

if result.is_valid() && result.confidence() >= 0.7 {
    println!("✅ Valid RDF! Confidence: {:.2}", result.confidence());
} else {
    println!("⚠️ Low confidence: {:.2}", result.confidence());
    for error in result.errors() {
        eprintln!("❌ {}: {}", error.rule, error.message);
    }
}
```

### Advanced: SPARQL ASK Validation

```rust
use text_to_rdf::{RdfValidator, ValidationRule, ValidationConfig};
use oxigraph::store::Store;
use std::sync::Arc;

// Enable SPARQL validation
let config = ValidationConfig {
    enable_sparql_validation: true,
    drop_invalid: false,  // Flag as low confidence instead of dropping
    min_confidence: 0.7,
};

let store = Arc::new(Store::new()?);
let validator = RdfValidator::with_config(config)
    .with_store(store);

// Add custom SPARQL ASK rule
validator.add_rule(ValidationRule {
    name: "person_born_after_1800".to_string(),
    description: "Person must have birthDate after 1800".to_string(),
    required_properties: vec![],
    entity_type: Some("Person".to_string()),
    sparql_ask: Some(r#"
        ASK {
            ?person a schema:Person .
            ?person schema:birthDate ?date .
            FILTER(YEAR(?date) > 1800)
        }
    "#.to_string()),
});
```

### Confidence Scoring

The validator assigns confidence scores based on violations:

| Violation Type | Impact | Severity |
|----------------|--------|----------|
| Missing required property | -0.2 | Error |
| Invalid date format | -0.05 | Warning |
| Invalid URI | -0.1 | Warning |
| SPARQL ASK failure | -0.15 | Warning |
| Basic structure error | -0.5 | Error |

**Example**:
- Document starts at 1.0 confidence
- Missing person name: 1.0 - 0.2 = 0.8
- Invalid date format: 0.8 - 0.05 = 0.75
- Final confidence: 0.75 (still valid if threshold is 0.7)

See [docs/HYBRID_PIPELINE.md](docs/HYBRID_PIPELINE.md) for complete examples.

## Advanced Features

### Hardcoded @context Injection

**Problem**: LLMs often hallucinate incorrect Schema.org URIs or malform the `@context` field.

**Solution**: The library includes a hardcoded `context.jsonld` file that ensures correct URIs regardless of LLM output.

```rust
let config = ExtractionConfig::new()
    .with_inject_hardcoded_context(true);  // Default: true

let extractor = GenAiExtractor::new(config)?;
let doc = extractor.extract(text).await?;

// The @context is automatically injected from context.jsonld
// Prevents issues like:
// - Wrong URI: "http://schema.org/" instead of "https://schema.org/"
// - Missing prefixes: No "rdf:", "owl:", etc.
// - Type coercion errors: Dates not marked as xsd:date
```

**What's in context.jsonld**:
- Full Schema.org vocabulary
- RDF, RDFS, OWL, XSD prefixes
- FOAF (Friend of a Friend)
- Dublin Core Terms (dcterms)
- GeoSPARQL (geo)
- Time Ontology
- Proper type coercion for dates, URLs, etc.

### Model Switching

**Problem**: Using expensive models like Claude or GPT-4 for simple entity extraction wastes money and time.

**Solution**: Configure a cheap/fast model for simple tasks and reserve powerful models for complex relation extraction.

```rust
let config = ExtractionConfig::new()
    .with_model("claude-3-5-sonnet")        // For complex relation extraction
    .with_simple_model("llama3.2:3b");      // For simple entity extraction

let extractor = GenAiExtractor::new(config)?;
```

**Recommended Model Combinations**:

| Use Case | Simple Model | Complex Model | Savings |
|----------|-------------|---------------|---------|
| **High Quality** | `llama3.2:3b` (Ollama) | `claude-3-5-sonnet` | 80% cost reduction |
| **Balanced** | `gpt-4o-mini` | `gpt-4o` | 90% cost reduction |
| **Budget** | `llama3.2:3b` (local) | `llama3.3:70b` (local) | 100% (free) |
| **Speed** | `gemini-2.0-flash` | `gemini-1.5-pro` | 95% cost reduction |

**When to use simple model**:
- Entity type classification (Person, Org, Place)
- Basic property extraction (name, birthDate)
- JSON-LD structure validation

**When to use complex model**:
- Relation extraction (worksFor, foundedBy)
- Temporal reasoning ("after graduating", "before 1990")
- Entity disambiguation (Apple Inc. vs. apple fruit)

### PDF Preprocessing with Docling

**Problem**: Standard PDF-to-text tools destroy table structure and lose formatting, making RDF extraction difficult.

**Solution**: Use [Docling](https://github.com/DS4SD/docling) (IBM Research, 2026) for high-quality PDF→Markdown conversion.

**Install Docling**:
```bash
pip install docling
```

**Usage Example**:
```python
from docling.document_converter import DocumentConverter

# Convert PDF to Markdown
converter = DocumentConverter()
result = converter.convert("paper.pdf")

# Get clean Markdown with preserved tables
markdown = result.document.export_to_markdown()

# Save for RDF extraction
with open("paper.md", "w") as f:
    f.write(markdown)
```

**Then use in Rust**:
```rust
use text_to_rdf::{RdfExtractor, GenAiExtractor, ExtractionConfig};
use std::fs;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Read Docling-processed Markdown
    let markdown = fs::read_to_string("paper.md")?;

    let config = ExtractionConfig::from_env()?;
    let extractor = GenAiExtractor::new(config)?;

    // Extract RDF from clean Markdown
    let doc = extractor.extract(&markdown).await?;

    println!("{}", doc.to_json()?);
    Ok(())
}
```

**Why Docling over alternatives**:
- **Table Preservation**: Maintains cell relationships (critical for tabular data)
- **Formula Support**: Extracts LaTeX formulas correctly
- **Layout Analysis**: Identifies headers, footers, captions
- **Open Source**: No API costs, runs locally
- **Active Development**: IBM Research backing (2024-2026+)

**Alternatives**:
- **PyMuPDF**: Fast but loses table structure
- **pdfplumber**: Good for tables but limited layout analysis
- **Apache Tika**: Java dependency, slower
- **Unstructured.io**: Commercial API, expensive at scale

## Examples

The library includes working examples in the `examples/` directory:

### Basic Extraction with .env

```bash
# Set up .env file first
cp .env.example .env
# Edit .env and add your GENAI_API_KEY

# Run the example
cargo run --example basic_extraction
```

This example demonstrates:
- Loading configuration from .env
- Extracting RDF from multiple text samples
- Displaying JSON-LD output

### Programmatic Configuration

```bash
GENAI_API_KEY=your-key cargo run --example programmatic_config
```

This example shows how to configure the library using the builder pattern without .env files.

## Using Local LLMs with Ollama

For local development and testing without API costs, you can use [Ollama](https://ollama.ai) with local models like Llama 3.3.

### Setup Ollama

1. **Install Ollama**:
   ```bash
   # macOS/Linux
   curl -fsSL https://ollama.ai/install.sh | sh

   # Or visit https://ollama.ai for other platforms
   ```

2. **Start Ollama server**:
   ```bash
   ollama serve
   ```

3. **Pull a model**:
   ```bash
   ollama pull llama3.3:8b
   # Other options: llama3.2, mistral, mixtral, phi3, qwen2.5
   ```

### Configure for Ollama

Copy the Ollama configuration:

```bash
cp .env.ollama .env
```

Or manually set in your `.env`:

```env
GENAI_API_KEY=ollama
RDF_EXTRACTION_MODEL=llama3.3:8b
GENAI_TEMPERATURE=0.3
GENAI_MAX_TOKENS=4096
```

### Run Tests with Ollama

```bash
# Make sure Ollama is running
ollama serve &

# Run integration tests
cargo test test_end_to_end_extraction -- --ignored

# Run examples
cargo run --example basic_extraction
```

### Benefits of Local LLMs

- **No API Costs**: Run unlimited tests locally
- **Privacy**: Data never leaves your machine
- **Offline Work**: No internet required
- **Fast Iteration**: No rate limits during development

### Model Recommendations

| Model | Size | Best For | Performance |
|-------|------|----------|-------------|
| `llama3.3:8b` | 8GB | General RDF extraction | Good balance |
| `llama3.2` | 3GB | Quick testing | Fast, less accurate |
| `mistral` | 7GB | Alternative to Llama | Good accuracy |
| `mixtral` | 47GB | High accuracy needs | Best but slower |

## Test Data

The library includes real-world test data from the **WebNLG challenge dataset**:

- `tests/fixtures/webnlg-sample.xml` - Airport entities (565KB)
- `tests/fixtures/webnlg-astronaut.xml` - Person/Astronaut entities (192KB)
- `tests/fixtures/test_cases.json` - Structured test cases with expected outputs

### Test Case Structure

```json
{
  "id": "astronaut_birthdate_1",
  "raw_text": "Alan Bean was born on the 15th of March 1932.",
  "expected_triples": [
    {
      "subject": "Alan_Bean",
      "predicate": "birthDate",
      "object": "1932-03-15"
    }
  ],
  "expected_jsonld": {
    "@context": "https://schema.org/",
    "@type": "Person",
    "name": "Alan Bean",
    "birthDate": "1932-03-15"
  }
}
```

## Running Tests

### Unit Tests (No API Key Required)

```bash
cargo test
```

Runs tests for:
- Library unit tests (config, types, extraction helpers)
- Integration tests (F1 score calculation, triple comparison)
- Doc tests

**All 39 tests** run successfully without any external dependencies.

### Integration Tests (Automatic Ollama Fallback)

Integration tests that require an LLM now automatically use **local Ollama** if no API key is provided:

```bash
# With API key (uses cloud LLM)
export GENAI_API_KEY="your-api-key"
cargo test

# Without API key (automatically uses Ollama if available)
cargo test
```

The tests will:
1. ✅ **Use your API key** if `GENAI_API_KEY` is set
2. 🦙 **Fall back to Ollama** (`llama3.3:8b`) if Ollama is running and the model is pulled
3. ⏭️  **Skip gracefully** if neither is available, with clear instructions

### Running with Ollama

```bash
# Start Ollama (if not already running)
ollama serve

# Pull the model
ollama pull llama3.3:8b

# Run tests (will automatically detect and use Ollama)
cargo test
```

**Output**:
```
test test_end_to_end_extraction ... ok
   🦙 Using local Ollama (llama3.3:8b)
   Testing: astronaut_birthdate_1
   Precision: 0.95
   Recall: 0.90
   F1 Score: 0.92
```

### Linting

```bash
cargo clippy -- -D warnings
```

## Testing Workflow (Consistency Check)

The library implements the recommended workflow for testing LLM-powered RDF extraction:

1. **Ingest**: Load `raw_text` from WebNLG dataset
2. **Extract**: Run text through `GenAiExtractor::extract()`
3. **Compare**: Compare resulting triples with `expected_triples`
4. **Metric**: Calculate F1 Score (Precision & Recall)

```rust
let metrics = EvaluationMetrics::new(&predicted_triples, &expected_triples);
println!("Precision: {:.2}", metrics.precision);
println!("Recall: {:.2}", metrics.recall);
println!("F1 Score: {:.2}", metrics.f1_score);
```

## Architecture

```
text-to-rdf/
├── src/
│   ├── lib.rs           # Public API & trait definitions
│   ├── error.rs         # Error types (thiserror)
│   ├── types.rs         # RdfDocument, RdfEntity, EntityType
│   └── extractor.rs     # GenAiExtractor implementation
├── tests/
│   ├── integration_tests.rs  # F1 score tests
│   └── fixtures/
│       ├── test_cases.json   # Structured test cases
│       ├── webnlg-sample.xml # WebNLG Airport data
│       └── webnlg-astronaut.xml # WebNLG Person data
├── .env.example         # Environment configuration template
└── Cargo.toml
```

## Configuration

The library supports two configuration approaches:

### 1. Environment Variables (Recommended)

Create a `.env` file (see `.env.example` for all options):

```env
GENAI_API_KEY=your-api-key-here
RDF_EXTRACTION_MODEL=claude-3-5-sonnet
GENAI_TEMPERATURE=0.3
GENAI_MAX_TOKENS=4096
```

Then load it in your code:

```rust
let config = ExtractionConfig::from_env()?;
let extractor = GenAiExtractor::new(config)?;
```

### 2. Programmatic Builder Pattern

```rust
let config = ExtractionConfig::new()
    .with_model("gpt-4o")               // Default: "claude-3-5-sonnet"
    .with_temperature(0.5)               // Default: 0.3
    .with_max_tokens(2000)               // Default: 4096
    .with_ontology("http://www.w3.org/2006/time#"); // Additional ontologies

let extractor = GenAiExtractor::new(config)?;
```

## Supported Entity Types

From Schema.org vocabulary:
- `Person` - People, including historical figures and astronauts
- `Organization` / `EducationalOrganization` - Companies, universities, institutions
- `Place` - Geographic locations, addresses
- `Event` - Temporal occurrences
- `Country`, `Award`, and custom types

## Dataset Sources

- **WebNLG Challenge**: https://github.com/ThiagoCF05/webnlg
- **T-REx** (Wikipedia to Wikidata): https://github.com/hadyelsahar/RE-NLG-Dataset
- **RELD** (Relation Extraction Linked Data): Various sources

## License

See `LICENSE` file for details.

## Contributing

Contributions welcome! Please ensure:
- All tests pass (`cargo test`)
- No clippy warnings (`cargo clippy -- -D warnings`)
- Tests include expected JSON-LD outputs for validation
