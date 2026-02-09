# Testing Guide

This guide covers different ways to test the text-to-rdf library using various LLM providers.

## Quick Start

### Option 1: Local Testing with Ollama (Recommended for Development)

**Pros**: Free, private, no rate limits
**Cons**: Requires local setup, may be less accurate than cloud models

```bash
# 1. Install and start Ollama
curl -fsSL https://ollama.ai/install.sh | sh
ollama serve &

# 2. Pull the model
ollama pull llama3.3:8b

# 3. Copy Ollama configuration
cp .env.ollama .env

# 4. Run tests
cargo test                                      # Unit tests
cargo test test_end_to_end_extraction -- --ignored  # Integration test
cargo run --example basic_extraction            # Examples
```

### Option 2: Cloud API Testing

**Pros**: High accuracy, no local setup
**Cons**: Requires API key, costs money, rate limits

```bash
# 1. Copy example config
cp .env.example .env

# 2. Edit .env and add your API key
# GENAI_API_KEY=your-actual-api-key-here

# 3. Choose your model
# RDF_EXTRACTION_MODEL=claude-3-5-sonnet    # Anthropic
# RDF_EXTRACTION_MODEL=gpt-4o               # OpenAI
# RDF_EXTRACTION_MODEL=gemini-1.5-pro       # Google

# 4. Run tests
cargo test test_end_to_end_extraction -- --ignored
```

## Test Types

### 1. Unit Tests (No API Key Required)

Tests the library logic without calling any LLM:

```bash
cargo test
```

**What it tests:**
- Configuration loading (ExtractionConfig)
- JSON-LD parsing and validation (RdfDocument)
- Triple extraction logic
- F1 score calculation
- Error handling

**Output:**
- 11 unit tests in `src/lib.rs`
- 5 integration tests (non-LLM parts)
- 2 doc tests
- **~18 tests total**

### 2. Integration Tests (API Key Required)

Tests actual RDF extraction with real LLMs using WebNLG dataset:

```bash
# Set up .env first (see Quick Start above)
cargo test test_end_to_end_extraction -- --ignored
```

**What it tests:**
- Full extraction pipeline
- JSON-LD output format
- Precision, Recall, F1 score
- Comparison with expected triples

**Test data:** `tests/fixtures/test_cases.json`

### 3. Examples

Run interactive examples:

```bash
cargo run --example basic_extraction        # Multiple extractions
cargo run --example programmatic_config     # Builder pattern config
cargo run --example pipeline_demo           # Entity linking & validation pipeline
```

### 4. Entity Linking Tests (Network Required)

Tests entity linking with DBpedia Spotlight (requires internet):

```bash
# Test entity linking integration
cargo test test_entity_linking_integration -- --ignored --nocapture

# Test validation with linked URIs
cargo test test_validation_with_linking
```

**What it tests:**
- DBpedia Spotlight API integration
- Entity name resolution to canonical URIs
- Confidence score thresholding
- URI enrichment in JSON-LD
- Validation with canonical URIs

**Note:** The public DBpedia Spotlight API may have availability issues. For production, consider:
- Self-hosting DBpedia Spotlight
- Using institutional instances
- Implementing Wikidata API integration

For more details, see `examples/ENTITY_LINKING.md`.

## Model Comparison

| Provider | Model | Setup | Speed | Accuracy | Cost |
|----------|-------|-------|-------|----------|------|
| **Ollama** | llama3.3:8b | Local | Fast | Good | Free |
| **Ollama** | mistral | Local | Fast | Good | Free |
| **Ollama** | mixtral | Local | Slow | High | Free |
| **Anthropic** | claude-3-5-sonnet | API key | Fast | High | $3/$15 per 1M tokens |
| **OpenAI** | gpt-4o | API key | Fast | High | $2.50/$10 per 1M tokens |
| **Google** | gemini-1.5-pro | API key | Fast | High | $1.25/$5 per 1M tokens |

## Configuration Files

### `.env` (Active Configuration)
Your current configuration - git ignored for security.

### `.env.example` (Cloud Template)
Template for cloud API providers. Includes all options.

### `.env.ollama` (Local Template)
Pre-configured for Ollama local testing.

```bash
# Switch between configurations
cp .env.example .env    # Use cloud APIs
cp .env.ollama .env     # Use local Ollama
```

## Environment Variables

| Variable | Required | Example | Description |
|----------|----------|---------|-------------|
| `GENAI_API_KEY` | Yes | `sk-abc123...` or `ollama` | API key or "ollama" for local |
| `RDF_EXTRACTION_MODEL` | No | `claude-3-5-sonnet` | Model identifier |
| `GENAI_TEMPERATURE` | No | `0.3` | Creativity (0.0-1.0) |
| `GENAI_MAX_TOKENS` | No | `4096` | Response length limit |
| `GENAI_SYSTEM_PROMPT` | No | Custom prompt | Override default instructions |
| `RDF_ONTOLOGIES` | No | `https://schema.org/,...` | Comma-separated URLs |
| `ENTITY_LINKING_ENABLED` | No | `true` or `false` | Enable entity linking (default: false) |
| `ENTITY_LINKING_STRATEGY` | No | `dbpedia`, `wikidata`, `none` | Linking service (default: none) |
| `ENTITY_LINKING_SERVICE_URL` | No | `https://api.dbpedia-spotlight.org/en` | DBpedia Spotlight URL |
| `ENTITY_LINKING_CONFIDENCE` | No | `0.5` | Confidence threshold 0.0-1.0 (default: 0.5) |

## Troubleshooting

### "GENAI_API_KEY environment variable is required"

**Solution:** Create `.env` file with your API key:
```bash
cp .env.example .env
# Edit .env and set GENAI_API_KEY
```

### Ollama connection refused

**Solution:** Make sure Ollama is running:
```bash
ollama serve
```

Check if it's running:
```bash
curl http://localhost:11434/api/version
```

### Model not found (Ollama)

**Solution:** Pull the model first:
```bash
ollama pull llama3.3:8b
ollama list  # Verify it's installed
```

### Tests timing out

**Solution:** Increase timeout or use faster model:
```env
RDF_EXTRACTION_MODEL=llama3.2      # Smaller, faster
GENAI_MAX_TOKENS=2048            # Shorter responses
```

### Low F1 scores in tests

**Known issue:** Local models may not match cloud model accuracy.

**Expected F1 scores:**
- Cloud models (Claude, GPT-4): 0.7-0.9
- Local models (Llama 3.3): 0.5-0.7
- Smaller local models: 0.3-0.5

The test threshold is set to `> 0.5` to accommodate local models.

## CI/CD Integration

### GitHub Actions Example

```yaml
name: Tests

on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      # Unit tests only (no API key needed)
      - name: Run unit tests
        run: cargo test

      # Integration tests with Ollama
      - name: Install Ollama
        run: curl -fsSL https://ollama.ai/install.sh | sh

      - name: Start Ollama and pull model
        run: |
          ollama serve &
          sleep 5
          ollama pull llama3.3:8b

      - name: Run integration tests
        run: |
          cp .env.ollama .env
          cargo test test_end_to_end_extraction -- --ignored
```

## Performance Benchmarks

Approximate time to extract one entity (on M1 MacBook Pro):

| Model | Extraction Time | Tokens/sec |
|-------|----------------|------------|
| llama3.3:8b (Ollama) | 2-3s | ~30 |
| mistral (Ollama) | 1-2s | ~50 |
| claude-3-5-sonnet | 1-2s | ~100 |
| gpt-4o | 1-2s | ~100 |

## Dataset Information

### WebNLG Challenge Dataset

The test fixtures use real data from the WebNLG challenge:

- **Source**: https://github.com/ThiagoCF05/webnlg
- **Format**: XML with RDF triples + natural language text
- **Categories**: Astronauts, Airports, Organizations
- **License**: CC BY-NC-SA 4.0

**Files:**
- `tests/fixtures/webnlg-astronaut.xml` (193KB) - Person entities
- `tests/fixtures/webnlg-sample.xml` (565KB) - Airport/Place entities
- `tests/fixtures/test_cases.json` (1.9KB) - Parsed test cases

## Contributing

When adding tests:
1. Keep unit tests free of external dependencies
2. Mark LLM-requiring tests with `#[ignore]`
3. Test with both cloud and local models
4. Document expected F1 score ranges
5. Use WebNLG or similar real-world data

## Further Reading

- [genai crate documentation](https://docs.rs/genai)
- [Ollama model library](https://ollama.ai/library)
- [Schema.org vocabulary](https://schema.org)
- [JSON-LD specification](https://json-ld.org/)
