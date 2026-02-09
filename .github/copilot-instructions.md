# Project: RDF Extraction Engine (Rust Library)

## 1. Project Overview & Goal
This is a high-performance Rust library designed to extract structured RDF data (entities and relations) from unstructured text and PDF documents. 
- **Core Goal**: Transform raw input into valid RDF triples/quads mapped to standard ontologies (Schema.org, GeoSPARQL, etc.).
- **Method**: Leveraging LLMs via the `genai` crate for Schema-First Extraction.

## 2. Tech Stack & Architecture
- **Language**: Rust (Latest stable edition).
- **Crate Focus**: `genai` for multi-provider AI orchestration (Gemini, Claude, GPT).
- **Format**: Library crate (`lib.rs`). Do not generate `main.rs` unless specifically asked for an example.
- **Async Runtime**: `tokio`.

## 3. Coding Standards (Rust)
- **Error Handling**: Use `thiserror` for library-level error variants and `anyhow` only in internal helpers or tests. Always return `Result<T, E>`.
- **Public API**: Use the `pub` keyword judiciously. Document all public items with `///` docstrings including a `# Errors` section where applicable.
- **Traits over Types**: Prefer defining traits for "Extractors" and "Resolvers" to allow for future extensibility.
- **Performance**: Prioritize zero-copy parsing where possible. Use `Cow<'a, str>` or `&str` for transient text data.
- **Building**: Ensure all code compile without warnings. Use `cargo clippy` in pedantic mode to enforce linting rules.
- **Documentation**: All public items must have comprehensive documentation. Include examples in docstrings where relevant, documentation should be always up to date, and should include system architecute and all pipelines diagrams.

## 4. AI & RDF Logic (The "Brain")
- **Gen-AI Usage**: Use `genai::Client` and `ChatRequest`. Always assume the model is "Smart" (e.g., Claude 3.5/4.5 or GPT-4o) and capable of JSON-LD output.
- **Schema-First Extraction**:
    - Always prompt the AI to return data in **JSON-LD** format.
    - Explicitly map extracted fields to **Schema.org** (`schema:`) and **RDF** (`rdf:`) namespaces.
- **Entity Resolution**: When merging new nodes, prioritize creating `owl:sameAs` relationships rather than over-writing existing nodes.

## 5. Domain Knowledge (Ontology)
- **Namespaces**:
    - `schema`: http://schema.org/
    - `rdf`: http://www.w3.org/1999/02/22-rdf-syntax-ns#
    - `geo`: http://www.opengis.net/ont/geosparql#
- **Standard Types**: Use `schema:Person`, `schema:Organization`, `schema:Place`, and `schema:Event` as the default classes.

## 6. Prompting Guidelines for Copilot
- **Avoid Hallucinations**: If a library or crate doesn't exist (check `genai` docs), do not invent functions. 
- **Context Awareness**: Before generating a new extractor, check if there is an existing `Trait` in the codebase to implement.
- **PDF Handling**: Assume PDF text extraction is handled by a separate module (e.g., `pdf-extract`). Focus the AI on the *interpretation* of that text.

## 7. Build & Test Commands
- **Check**: `cargo check`
- **Test**: `cargo test`
- **Lint**: `cargo clippy -- -D warnings`
