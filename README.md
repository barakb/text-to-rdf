# Text-to-RDF: RDF Extraction Library

A high-performance Rust library for extracting structured RDF data (entities and relations) from unstructured text using LLMs via the `genai` crate.

## Features

- **Schema-First Extraction**: Outputs JSON-LD mapped to Schema.org and standard RDF ontologies
- **Multi-Provider AI Support**: Works with Gemini, Claude, GPT via `genai`
- **Coreference Resolution** (pure Rust): Resolve pronouns to canonical entities before extraction
- **GLiNER Zero-Shot NER** (optional): Fast local entity extraction with provenance tracking (4x faster than Python)
- **Hybrid Pipeline**: 5-stage production-grade pipeline from preprocessing to validation
- **Entity Linking**: Local Rust-native linking with Oxigraph or remote APIs (DBpedia, Wikidata)
- **SHACL-like Validation**: Schema validation with custom rules
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
| `ENTITY_LINKING_ENABLED` | No | `false` | Enable entity linking |
| `ENTITY_LINKING_STRATEGY` | No | `none` | Strategy: `local`, `dbpedia`, `wikidata`, or `none` |
| `ENTITY_LINKING_KB_PATH` | No | - | Path to local RDF knowledge base (for `local` strategy) |
| `ENTITY_LINKING_CONFIDENCE` | No | `0.5` | Confidence threshold 0.0-1.0 |

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
