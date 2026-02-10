# Roadmap to 70%+ F1 Score: Production-Grade RDF Extraction

**Current State**: 39.68% F1 (qwen2.5:7b on DocRED)
**Target**: 70-80% F1 (Production-ready)
**Status**: Phase 0 Complete ✅ (Model evaluation)

---

## 📊 Gap Analysis

| Component | Status | Impact on F1 | Priority |
|-----------|--------|--------------|----------|
| Model evaluation | ✅ Complete | +0% (baseline) | - |
| Contextual sliding windows | ❌ Missing | +15-20% | 🔴 Critical |
| Knowledge buffer | ❌ Missing | +10-15% | 🔴 Critical |
| Native coreference resolution | ❌ Missing | +8-12% | 🟡 High |
| Provenance metadata | ❌ Missing | +2-5% | 🟢 Medium |
| Streaming triples | ❌ Missing | +0% (perf only) | 🟢 Medium |
| Parallel processing | ❌ Missing | +0% (perf only) | 🟢 Medium |

**Total Expected Improvement**: +35-52% F1 → **75-92% F1** 🎯

---

## 🚀 Phase 1: Document Context (2-3 days) → +25-35% F1

**Goal**: Fix the "lost context" problem in document-level extraction

### 1.1 Semantic Chunking with `text-splitter-rs`

**Problem**: Current system loses entity references across paragraphs.

**Solution**: Implement semantic boundary-aware chunking.

**New File**: `src/chunking.rs`

```rust
use text_splitter::{ChunkConfig, MarkdownSplitter};

pub struct SemanticChunker {
    max_chunk_size: usize,
    overlap_sentences: usize,
}

impl SemanticChunker {
    /// Split text at semantic boundaries (headers, paragraphs, sentences)
    pub fn chunk(&self, text: &str) -> Vec<DocumentChunk> {
        let splitter = MarkdownSplitter::new(ChunkConfig::new(self.max_chunk_size)
            .with_sizer(ChunkSizer::CharacterCount)
            .with_overlap(200)); // 200 char overlap

        let chunks = splitter.chunks(text);

        chunks.enumerate().map(|(idx, chunk)| DocumentChunk {
            id: idx,
            text: chunk.to_string(),
            start_offset: chunk.start(),
            end_offset: chunk.end(),
            entities_mentioned: vec![],
        }).collect()
    }
}

pub struct DocumentChunk {
    pub id: usize,
    pub text: String,
    pub start_offset: usize,
    pub end_offset: usize,
    pub entities_mentioned: Vec<String>,
}
```

**Integration**: Update `extractor.rs` to use chunking for documents > 2000 tokens.

**Expected Impact**: +8-12% F1

---

### 1.2 Knowledge Buffer (Entity Context Preservation)

**Problem**: Chunk 3 doesn't know about entities discovered in Chunk 1.

**Solution**: Pass a "knowledge buffer" to each chunk extraction.

**New File**: `src/knowledge_buffer.rs`

```rust
use std::collections::HashMap;

/// Tracks entities discovered across document chunks
pub struct KnowledgeBuffer {
    entities: HashMap<String, EntityContext>,
}

#[derive(Debug, Clone)]
pub struct EntityContext {
    pub canonical_name: String,
    pub entity_type: String,
    pub first_mention_offset: usize,
    pub aliases: Vec<String>,
    pub properties: HashMap<String, String>,
}

impl KnowledgeBuffer {
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
        }
    }

    /// Add an entity discovered in a chunk
    pub fn add_entity(&mut self, name: &str, entity_type: &str, offset: usize) {
        self.entities.entry(name.to_string()).or_insert(EntityContext {
            canonical_name: name.to_string(),
            entity_type: entity_type.to_string(),
            first_mention_offset: offset,
            aliases: vec![],
            properties: HashMap::new(),
        });
    }

    /// Register an alias (e.g., "the company" → "Apple Inc.")
    pub fn add_alias(&mut self, alias: &str, canonical: &str) {
        if let Some(entity) = self.entities.get_mut(canonical) {
            if !entity.aliases.contains(&alias.to_string()) {
                entity.aliases.push(alias.to_string());
            }
        }
    }

    /// Get entity context for prompt injection
    pub fn get_context_summary(&self) -> String {
        let mut summary = String::from("ENTITIES MENTIONED SO FAR:\n");
        for (name, ctx) in &self.entities {
            summary.push_str(&format!(
                "- {} ({}): {}\n",
                name,
                ctx.entity_type,
                ctx.aliases.join(", ")
            ));
        }
        summary
    }

    /// Resolve an alias to canonical name
    pub fn resolve_alias(&self, text: &str) -> Option<String> {
        for (canonical, ctx) in &self.entities {
            if ctx.aliases.iter().any(|a| a.eq_ignore_ascii_case(text)) {
                return Some(canonical.clone());
            }
        }
        None
    }
}
```

**Integration**: Inject knowledge buffer into system prompt for each chunk:

```rust
let context_prompt = format!(
    "{}\n\n{}\n\nExtract relations from this section:",
    base_prompt,
    knowledge_buffer.get_context_summary()
);
```

**Expected Impact**: +10-15% F1

---

### 1.3 Multi-Chunk Extraction Pipeline

**Update**: `src/extractor.rs`

```rust
/// Extract from long document using chunking + knowledge buffer
pub async fn extract_from_document(&self, text: &str) -> Result<RdfDocument> {
    // 1. Check if document needs chunking
    let token_count = self.estimate_tokens(text);

    if token_count < 2000 {
        // Short document - extract normally
        return self.extract(text).await;
    }

    // 2. Semantic chunking
    let chunker = SemanticChunker::new(1500, 2);
    let chunks = chunker.chunk(text);

    // 3. Knowledge buffer for entity tracking
    let mut kb = KnowledgeBuffer::new();
    let mut all_triples = Vec::new();

    // 4. Process chunks sequentially (preserve order for coreference)
    for chunk in chunks {
        // Inject knowledge buffer context
        let context_prompt = self.build_context_prompt(&kb);
        let chunk_doc = self.extract_with_context(&chunk.text, &context_prompt).await?;

        // Update knowledge buffer with discovered entities
        for entity in &chunk_doc.entities {
            kb.add_entity(&entity.name, &entity.entity_type, chunk.start_offset);
        }

        // Collect triples
        all_triples.extend(chunk_doc.triples);
    }

    // 5. Merge and deduplicate
    Ok(self.merge_chunks(all_triples))
}
```

**Expected Impact**: +7-8% F1 (combined with above)

---

## 🎯 Phase 2: Coreference Resolution (1-2 days) → +8-12% F1

**Goal**: Resolve "he", "she", "the company", "it" to canonical entities.

### 2.1 Implement Rule-Based Coreference

**New File**: `src/coreference.rs`

```rust
use regex::Regex;
use std::collections::HashMap;

pub struct CoreferenceResolver {
    pronoun_patterns: HashMap<String, Vec<String>>,
}

impl CoreferenceResolver {
    pub fn new() -> Self {
        let mut pronoun_patterns = HashMap::new();

        // Define pronoun patterns by entity type
        pronoun_patterns.insert(
            "Person".to_string(),
            vec!["he", "she", "him", "her", "his", "hers"]
                .into_iter()
                .map(String::from)
                .collect(),
        );
        pronoun_patterns.insert(
            "Organization".to_string(),
            vec!["it", "its", "the company", "the firm", "the organization"]
                .into_iter()
                .map(String::from)
                .collect(),
        );

        Self { pronoun_patterns }
    }

    /// Resolve pronouns in text using knowledge buffer
    pub fn resolve(&self, text: &str, kb: &KnowledgeBuffer) -> String {
        let mut resolved = text.to_string();

        // Find the most recent entity of each type
        let mut last_person = kb.get_last_entity_of_type("Person");
        let mut last_org = kb.get_last_entity_of_type("Organization");

        // Replace pronouns with canonical names
        for (entity_type, pronouns) in &self.pronoun_patterns {
            let canonical = match entity_type.as_str() {
                "Person" => &last_person,
                "Organization" => &last_org,
                _ => continue,
            };

            if let Some(canonical_name) = canonical {
                for pronoun in pronouns {
                    let re = Regex::new(&format!(r"\b{}\b", regex::escape(pronoun))).unwrap();
                    resolved = re.replace_all(&resolved, canonical_name.as_str()).to_string();
                }
            }
        }

        resolved
    }
}
```

**Integration**: Apply before extraction:

```rust
let coref_resolver = CoreferenceResolver::new();
let resolved_text = coref_resolver.resolve(&chunk.text, &kb);
let chunk_doc = self.extract(&resolved_text).await?;
```

**Expected Impact**: +8-12% F1

---

## 🔗 Phase 3: Entity Linking (1 day) → +3-6% F1

**Goal**: Map string names to canonical Wikidata/DBpedia URIs.

### 3.1 Local Wikidata Index with Oxigraph

**New File**: `src/entity_linking_enhanced.rs`

```rust
use oxigraph::store::Store;
use std::sync::Arc;

pub struct WikidataLinker {
    store: Arc<Store>,
}

impl WikidataLinker {
    /// Load a local Wikidata SPARQL index
    pub fn new(kb_path: &str) -> Result<Self> {
        let store = Store::open(kb_path)?;
        Ok(Self {
            store: Arc::new(store),
        })
    }

    /// Find Wikidata URI for entity name
    pub fn link_entity(&self, name: &str, entity_type: &str) -> Option<String> {
        let query = format!(
            r#"
            PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
            PREFIX wd: <http://www.wikidata.org/entity/>

            SELECT ?entity WHERE {{
                ?entity rdfs:label "{}"@en .
                ?entity wdt:P31 ?type .
                FILTER(CONTAINS(STR(?type), "{}"))
            }}
            LIMIT 1
            "#,
            name, entity_type
        );

        // Execute SPARQL query
        self.store
            .query(&query)
            .ok()?
            .into_iter()
            .next()
            .and_then(|solution| solution.get("entity").map(|v| v.to_string()))
    }
}
```

**Integration**: Add URIs to extracted entities:

```rust
for entity in &mut doc.entities {
    if let Some(uri) = wikidata_linker.link_entity(&entity.name, &entity.entity_type) {
        entity.id = Some(uri);
    }
}
```

**Expected Impact**: +3-6% F1 (reduces duplicate entities)

---

## 📍 Phase 4: Provenance Tracking (1 day) → +2-5% F1

**Goal**: Track which text span supported each triple.

### 4.1 Add Provenance Metadata

**Update**: `src/types.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RdfTriple {
    pub subject: String,
    pub predicate: String,
    pub object: String,

    // NEW: Provenance metadata
    pub provenance: Option<Provenance>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Provenance {
    /// Character offset in source document
    pub text_span: (usize, usize),

    /// Confidence score (0.0-1.0)
    pub confidence: f64,

    /// Source chunk ID (for multi-chunk documents)
    pub chunk_id: Option<usize>,

    /// Extraction method used
    pub method: String, // "llm", "gliner", "rule-based"
}
```

**Integration**: Track offsets during extraction:

```rust
let triple = RdfTriple {
    subject: "Marie Curie".to_string(),
    predicate: "birthPlace".to_string(),
    object: "Warsaw".to_string(),
    provenance: Some(Provenance {
        text_span: (245, 289), // "Marie Curie was born in Warsaw"
        confidence: 0.95,
        chunk_id: Some(0),
        method: "llm".to_string(),
    }),
};
```

**Expected Impact**: +2-5% F1 (enables debugging and filtering)

---

## ⚡ Phase 5: Performance Optimizations (2 days) → +0% F1, 4x faster

**Goal**: Make extraction fast enough for production.

### 5.1 Streaming Triples Architecture

**New File**: `src/streaming.rs`

```rust
use tokio::sync::mpsc;
use futures::Stream;

pub struct StreamingExtractor {
    extractor: GenAiExtractor,
}

impl StreamingExtractor {
    /// Extract triples as a stream (don't wait for full document)
    pub async fn extract_stream(
        &self,
        text: &str,
    ) -> impl Stream<Item = Result<RdfTriple>> {
        let (tx, rx) = mpsc::channel(100);

        let extractor = self.extractor.clone();
        let text = text.to_string();

        tokio::spawn(async move {
            let chunks = chunk_document(&text);

            for chunk in chunks {
                match extractor.extract(&chunk.text).await {
                    Ok(doc) => {
                        for triple in doc.triples {
                            let _ = tx.send(Ok(triple)).await;
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(Err(e)).await;
                    }
                }
            }
        });

        tokio_stream::wrappers::ReceiverStream::new(rx)
    }
}
```

**Usage**:

```rust
let mut stream = extractor.extract_stream(document).await;

while let Some(triple) = stream.next().await {
    match triple {
        Ok(t) => db.insert_triple(t).await?,
        Err(e) => eprintln!("Error: {}", e),
    }
}
```

**Expected Impact**: 4x faster ingestion (overlaps I/O with processing)

---

### 5.2 Parallel Processing with Rayon

**Update**: `src/validation.rs`

```rust
use rayon::prelude::*;

impl RdfValidator {
    /// Validate triples in parallel
    pub fn validate_parallel(&self, triples: Vec<RdfTriple>) -> ValidationResult {
        let results: Vec<_> = triples
            .par_iter() // Rayon parallel iterator
            .map(|triple| self.validate_triple(triple))
            .collect();

        self.merge_results(results)
    }
}
```

**Expected Impact**: 3x faster validation

---

## 📋 Implementation Schedule

### Week 1: Document Context (Phase 1)
- **Day 1**: Semantic chunking (`src/chunking.rs`)
- **Day 2**: Knowledge buffer (`src/knowledge_buffer.rs`)
- **Day 3**: Multi-chunk pipeline integration
- **Expected**: 39% → 64% F1 (+25%)

### Week 2: Coreference + Entity Linking (Phases 2-3)
- **Day 4**: Rule-based coreference (`src/coreference.rs`)
- **Day 5**: Wikidata entity linking (`src/entity_linking_enhanced.rs`)
- **Expected**: 64% → 75% F1 (+11%)

### Week 3: Provenance + Performance (Phases 4-5)
- **Day 6**: Provenance tracking (`src/types.rs`)
- **Day 7**: Streaming architecture (`src/streaming.rs`)
- **Day 8**: Parallel processing with Rayon
- **Expected**: 75% F1 (stable), 4x faster

---

## 🎯 Success Metrics

| Milestone | F1 Score | Speed (docs/sec) | Status |
|-----------|----------|------------------|--------|
| Baseline | 39.68% | 0.5 | ✅ Complete |
| Phase 1 | 60-70% | 0.5 | 🔄 In Progress |
| Phase 2 | 68-78% | 0.5 | ⏳ Pending |
| Phase 3 | 70-80% | 0.5 | ⏳ Pending |
| Phase 4 | 72-82% | 0.5 | ⏳ Pending |
| Phase 5 | 72-82% | 2.0 | ⏳ Pending |

**Target**: 70%+ F1 at 2+ docs/sec → **Production-Ready** ✅

---

## 🛠️ Dependencies to Add

```toml
[dependencies]
# Semantic chunking
text-splitter = "0.16"

# Streaming
tokio = { version = "1.0", features = ["full"] }
tokio-stream = "0.1"
futures = "0.3"

# Parallel processing
rayon = "1.10"

# Regex for coreference
regex = "1.10"

# Already have:
# oxigraph, serde_json, genai, anyhow
```

---

## 📊 Evaluation Plan

After each phase, run:

```bash
# Test on DocRED
export RDF_EXTRACTION_MODEL=qwen2.5:7b
cargo run --example docred_evaluation

# Test on WebNLG (should remain 100% F1)
cargo run --example webnlg_evaluation
```

**Success Criteria**:
- DocRED F1 ≥ 70%
- WebNLG F1 = 100% (no regression)
- Processing speed ≥ 2 docs/sec

---

## 🚀 Let's Get Started!

**Next Step**: Implement Phase 1.1 (Semantic Chunking)

```bash
cargo add text-splitter
cargo add tokio --features full
cargo add regex
```

Then create `src/chunking.rs` and integrate with `src/extractor.rs`.
