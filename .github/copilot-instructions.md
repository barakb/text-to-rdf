# Project: RDF Extraction Engine (Rust Library)

---
description: Unified instructions for text-to-rdf extraction engine
globs: ["**/*.rs", "**/Cargo.toml", "context.jsonld"]
alwaysApply: true
---

# Project: text-to-rdf (High-Fidelity Semantic Engine)

## 1. Core Mission & Strategy
The goal is to extract structured RDF from text/PDFs using a **Local-First, Hybrid Pipeline**. We avoid brittle public APIs (like DBpedia Spotlight) in favor of deterministic local logic and high-reasoning LLMs.
Using only Rust code.

## 2. The Hybrid Extraction Pipeline
When writing extraction logic, follow this sequence:
1. **Discovery (GLiNER)**: Use Zero-Shot NER (`gline-rs`) to find entity spans. This is faster and more precise than LLM-only discovery.
2. **URI Linking (Local Index)**: 
   - Perform lookups in a local `oxigraph` store containing common Wikidata QIDs.
   - Use fuzzy matching (e.g., Levenshtein) for label alignment.
3. **Relation Extraction (genai)**: Use the `genai` crate to determine the predicates between discovered URIs.
4. **Validation**: Use **SHACL-like logic** or SPARQL `ASK` queries via Oxigraph to ensure output validity before finalizing the graph.

## 3. Rust Implementation Rules
- **Error Handling**: Implement `thiserror` for library-level error variants. Methods should return `Result<T, E>`.
- **Async Execution**: Use `tokio` for all LLM and I/O tasks. 
- **Efficiency**: 
    - Use `Cow<'a, str>` or `Arc<str>` for entity labels to minimize cloning.
    - Leverage `SmallVec` for triple collections when the count is likely < 8.
- **Graph Safety**: Use the `oxigraph` crate to build and manipulate the graph. **Never** use raw string concatenation to generate Turtle/N-Triples.
- **Dependency Management**: Only use crates that are actively maintained and have no known security issues. Avoid large, monolithic frameworks, try to use latest stable version of each dependency.

## 4. LLM & RDF Logic
- **Structured Output**: Force the LLM to return **JSON-LD**. 
- **Context Management**: Inject the project's `@context` (referencing `context.jsonld`) programmatically in Rust. Do not rely on the LLM to generate the `@context` block.
- **Ontology Mappings**:
    - Default to **Schema.org** for core entities (`Person`, `Organization`, `Place`).
    - Use **GeoSPARQL** for spatial data (`geo:asWKT`).
    - Use `owl:sameAs` to link internal entity nodes to Wikidata URIs.

## 5. Development Workflow
- **Validation**: Every public function must be documented with `///` and include a `# Errors` section.
- **Testing**: Include unit tests for extraction logic using small, deterministic text samples.
- **Commands**:
    - Format: `cargo fmt --all`
    - Build: `cargo build`
    - Test: `cargo test`
    - Lint: `cargo clippy --lib -- -W clippy::pedantic -W clippy::nursery -D warnings` 
    - Run all integration tests with real LLMs: `cargo test -- --ignored`
    - Run all examples: `cargo run --example <example_name>`

## 6. Prohibited Patterns
- ❌ Do not use external REST APIs for entity linking (e.g., public DBpedia Spotlight).
- ❌ Do not use `openai` or `anthropic` crates directly; use the `genai` abstraction.
- ❌ Do not write `fn main()` in library files; keep it in `examples/` or `tests/`.



## 7. Prompting Guidelines for Copilot
- **Avoid Hallucinations**: If a library or crate doesn't exist (check `genai` docs), do not invent functions.
- **Context Awareness**: Before generating a new extractor, check if there is an existing `Trait` in the codebase to implement.
- **Document**: Keep all documents always up to date.
