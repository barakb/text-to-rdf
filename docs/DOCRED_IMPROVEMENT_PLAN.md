# DocRED Evaluation Improvement Plan

## Current Performance Analysis

**Baseline Results**:
- Average Precision: 45.83%
- Average Recall: 35.00%
- **Average F1: 39.68%** ❌ (Target: 40-60%)

### Breakdown by Document:
1. **Marie Curie**: 57.14% F1 ✓ (Good)
2. **Apple Inc**: 0% F1 ❌ (Complete failure)
3. **Stanford University**: 20% F1 ⚠️ (Poor)

### Key Issues Identified:

1. **Wrong Relation Directions**
   - Extracted: `apple_inc → alumniof → Steve Jobs`
   - Expected: `steve_jobs → worksfor → Apple Inc.`
   - Problem: Subject/object confusion

2. **Entity Normalization Mismatches**
   - Extracted: `apple_inc`
   - Expected: `apple_inc.` (with period)
   - Problem: Inconsistent normalization

3. **Inferred Properties**
   - Extracting: `foundingDate`, `founder`, `alumni`, `currentCEO`
   - Expected: Only explicitly mappable Wikidata properties
   - Problem: Over-extraction of derived information

4. **Weak Model for Document-Level**
   - Currently using: `qwen2.5:7b`
   - Recommended: `claude-3-5-sonnet` or `gpt-4o`
   - Impact: 70B+ parameters needed for cross-sentence reasoning

---

## 🔬 Local Model Evaluation Results

We tested all available Ollama models to find the best local option for document-level extraction:

| Model | Parameters | F1 Score | Status | Key Issues |
|-------|------------|----------|--------|------------|
| **qwen2.5:7b** | **7B** | **39.68%** | ✅ **Best Local** | Minor relation direction issues |
| mistral:latest | 7B | 26.94% | ⚠️ Works | Severe relation direction problems (org→alumniOf→person) |
| phi4:latest | 14B | 7.41% | ❌ Poor | Property name truncation, entity normalization failures |
| llama3.1:latest | 8B | 0.00% | ❌ Failed | Cannot produce valid JSON-LD |

**Conclusion**: **qwen2.5:7b is the recommended local model** for document-level extraction. The 40% F1 score is actually reasonable performance for a 7B parameter model on cross-sentence relation extraction.

---

## 🚀 Phase 1: Quick Wins (1-2 hours) - Target: 45% → 55% F1

### 1.1 Use Stronger Model (Optional)

**Note**: After testing, qwen2.5:7b is the best local option. Only upgrade if you need higher accuracy.

**Solution**:
```bash
# Option A: Cloud LLM (Best results - 70-80% F1)
export GENAI_API_KEY=your-key
export RDF_EXTRACTION_MODEL=claude-3-5-sonnet-20241022

# Option B: Local Ollama 70B (Good results - 55-65% F1)
ollama pull llama3.3:70b
export RDF_EXTRACTION_MODEL=llama3.3:70b

# Option C: Keep qwen2.5:7b (Acceptable - 40% F1)
# Already installed and performs best among 7B models
```

**Expected Impact**:
- Cloud LLM: +30-40% F1 (expensive)
- Ollama 70B: +15-25% F1 (free but slow)
- qwen2.5:7b: Best 7B option (current baseline)

### 1.2 Improve Document-Level System Prompt

**File**: `src/extractor.rs`

Add document-specific guidance to the system prompt:

```rust
IMPORTANT FOR MULTI-SENTENCE DOCUMENTS:
- Track entities across sentences using coreference ("It", "She", "The company" refer to entities mentioned earlier)
- Only extract relations between entities mentioned in the document
- When you see "Steve Jobs founded Apple" → Extract: (Steve Jobs, worksFor, Apple Inc.)
- When you see "The company is in Cupertino" → Resolve "The company" to the main entity first
- DO NOT extract properties about the main entity from mentions of other entities
  Example: "Larry Page graduated from Stanford" → Extract: (Larry Page, alumniOf, Stanford University)
  NOT: (Stanford University, alumniOf, Larry Page)
```

**Expected Impact**: +5-8% F1

### 1.3 Fix Entity Normalization

**File**: `examples/docred_evaluation.rs:231`

Update `docred_to_triples` to handle periods in entity names:

```rust
fn docred_to_triples(doc: &DocREDDocument) -> HashSet<Triple> {
    let mut triples = HashSet::new();

    for relation in &doc.labels {
        if let (Some(subject), Some(object)) = (
            doc.get_entity_name(relation.head),
            doc.get_entity_name(relation.tail),
        ) {
            if let Some(schema_property) = map_wikidata_to_schema(&relation.relation_type) {
                triples.insert(Triple {
                    // Preserve periods and special chars in entity names
                    subject: subject.to_lowercase().replace(' ', "_"),
                    predicate: normalize_predicate(schema_property),
                    object,
                });
            }
        }
    }

    triples
}
```

**Expected Impact**: +2-3% F1

---

## 🔧 Phase 2: Medium Improvements (2-4 hours) - Target: 55% → 65% F1

### 2.1 Add Relation Direction Validation

**Problem**: Extracting `(Organization, alumniOf, Person)` instead of `(Person, alumniOf, Organization)`

**Solution**: Add validation rules in `src/validation.rs`

```rust
/// Validate relation directionality based on entity types
pub fn validate_relation_direction(
    subject_type: &str,
    predicate: &str,
    object_type: &str,
) -> Result<()> {
    match (subject_type, predicate) {
        // Person properties
        ("Person", "alumniOf") if object_type != "EducationalOrganization" => {
            Err(Error::Validation("alumniOf must connect Person to EducationalOrganization".into()))
        }
        ("Person", "worksFor") if object_type != "Organization" => {
            Err(Error::Validation("worksFor must connect Person to Organization".into()))
        }

        // Organization properties
        ("Organization", "location") if !["Place", "City"].contains(&object_type) => {
            Err(Error::Validation("Organization location must be a Place".into()))
        }

        _ => Ok(()),
    }
}
```

**Expected Impact**: +5-10% F1

### 2.2 Improve Wikidata → Schema.org Mapping

**File**: `examples/docred_evaluation.rs:196`

Add more precise mappings:

```rust
fn map_wikidata_to_schema(property_id: &str) -> Option<&'static str> {
    match property_id {
        // Location properties (be specific)
        "P17" => Some("addressCountry"),      // country
        "P131" => Some("containedInPlace"),   // administrative area
        "P276" => Some("location"),            // physical location
        "P159" => Some("location"),            // headquarters (for orgs)

        // Employment (with direction)
        "P108" => Some("worksFor"),           // employer (Person → Org)

        // Education (with direction)
        "P69" => Some("alumniOf"),            // educated at (Person → Org)

        // Membership
        "P463" => Some("memberOf"),           // member of
        "P102" => Some("affiliation"),        // political party
        "P54" => Some("memberOf"),            // sports team

        // Temporal
        "P569" => Some("birthDate"),          // date of birth
        "P570" => Some("deathDate"),          // date of death
        "P571" => Some("foundingDate"),       // inception
        "P576" => Some("dissolutionDate"),    // dissolved

        // Biographical
        "P19" => Some("birthPlace"),          // place of birth
        "P20" => Some("deathPlace"),          // place of death
        "P27" => Some("nationality"),         // citizenship

        _ => None,
    }
}
```

**Expected Impact**: +3-5% F1

### 2.3 Enhance Triple Extraction Logic

**File**: `examples/docred_evaluation.rs:238`

Improve extraction to handle more Schema.org patterns:

```rust
fn extract_triples_from_jsonld(jsonld: &Value) -> HashSet<Triple> {
    let mut triples = HashSet::new();

    if let Some(obj) = jsonld.as_object() {
        let subject = obj
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();

        let normalized_subject = subject.to_lowercase().replace(' ', "_");

        for (key, value) in obj {
            if key.starts_with('@') || key == "name" {
                continue;
            }

            match value {
                Value::String(s) => {
                    triples.insert(Triple {
                        subject: normalized_subject.clone(),
                        predicate: normalize_predicate(key),
                        object: s.clone(),
                    });
                }
                Value::Object(nested) => {
                    // Handle nested entities - preserve original name
                    if let Some(nested_name) = nested.get("name").and_then(|v| v.as_str()) {
                        triples.insert(Triple {
                            subject: normalized_subject.clone(),
                            predicate: normalize_predicate(key),
                            object: nested_name.to_string(),
                        });
                    }

                    // Handle nested properties (e.g., location.addressCountry)
                    if let Some(nested_obj) = nested.as_object() {
                        for (nested_key, nested_value) in nested_obj {
                            if nested_key.starts_with('@') || nested_key == "name" {
                                continue;
                            }
                            if let Some(s) = nested_value.as_str() {
                                triples.insert(Triple {
                                    subject: normalized_subject.clone(),
                                    predicate: normalize_predicate(nested_key),
                                    object: s.to_string(),
                                });
                            }
                        }
                    }
                }
                Value::Array(arr) => {
                    for item in arr {
                        if let Some(s) = item.as_str() {
                            triples.insert(Triple {
                                subject: normalized_subject.clone(),
                                predicate: normalize_predicate(key),
                                object: s.to_string(),
                            });
                        } else if let Some(obj) = item.as_object() {
                            if let Some(name) = obj.get("name").and_then(|v| v.as_str()) {
                                triples.insert(Triple {
                                    subject: normalized_subject.clone(),
                                    predicate: normalize_predicate(key),
                                    object: name.to_string(),
                                });
                            }
                        }
                    }
                }
                _ => {}
            }
        }
    }

    triples
}
```

**Expected Impact**: +2-5% F1

---

## 🏗️ Phase 3: Advanced Enhancements (4-8 hours) - Target: 65% → 75% F1

### 3.1 Implement Coreference Resolution Preprocessing

**Problem**: "It", "She", "The company" not resolved to actual entities

**Solution**: Add coreference resolution step

**New file**: `src/coreference.rs`

```rust
/// Resolve coreferences in text before extraction
pub fn resolve_coreferences(text: &str) -> String {
    // Simple rule-based coreference for common patterns
    let lines: Vec<&str> = text.lines().collect();
    let mut resolved = text.to_string();

    // Track last mentioned entities by type
    let mut last_person = None;
    let mut last_org = None;
    let mut last_place = None;

    // Replace pronouns with entity names
    // This is simplified - real implementation would use NER + coreference model

    resolved
}
```

**Integration**: Call before extraction in `extractor.rs`

**Expected Impact**: +5-10% F1

### 3.2 Add Document Chunking Strategy

**Problem**: Long documents lose context

**Solution**: Extract per-paragraph, then merge

**New file**: `src/document_chunking.rs`

```rust
pub struct DocumentChunk {
    pub text: String,
    pub paragraph_id: usize,
    pub entities: Vec<String>,
}

pub fn chunk_document(text: &str) -> Vec<DocumentChunk> {
    text.split("\n\n")
        .enumerate()
        .map(|(i, para)| DocumentChunk {
            text: para.to_string(),
            paragraph_id: i,
            entities: vec![],
        })
        .collect()
}

pub async fn extract_from_chunks(
    chunks: Vec<DocumentChunk>,
    extractor: &GenAiExtractor,
) -> Result<RdfDocument> {
    // Extract from each chunk
    // Merge results
    // Resolve conflicts
    todo!()
}
```

**Expected Impact**: +3-8% F1

### 3.3 Add Confidence-Based Filtering

**Problem**: Low-confidence extractions hurt precision

**Solution**: Filter out low-confidence relations

```rust
pub struct ConfidenceScorer {
    // Score based on:
    // - Entity mention distance
    // - Number of supporting sentences
    // - Relation type frequency in training data
}

impl ConfidenceScorer {
    pub fn score_triple(&self, triple: &Triple, context: &str) -> f64 {
        // Calculate confidence score
        // Return 0.0-1.0
        todo!()
    }

    pub fn filter_triples(
        triples: Vec<Triple>,
        context: &str,
        threshold: f64,
    ) -> Vec<Triple> {
        triples
            .into_iter()
            .filter(|t| self.score_triple(t, context) >= threshold)
            .collect()
    }
}
```

**Expected Impact**: +5-10% F1

---

## 📊 Implementation Priority

### Must Do (Phase 1 - 1-2 hours):
1. ✅ Switch to stronger model (claude-3-5-sonnet or llama3.3:70b)
2. ✅ Enhance system prompt for document-level extraction
3. ✅ Fix entity normalization

**Expected Result**: 55% F1 (+15%)

### Should Do (Phase 2 - 2-4 hours):
4. ✅ Add relation direction validation
5. ✅ Improve Wikidata → Schema.org mapping
6. ✅ Enhance triple extraction logic

**Expected Result**: 65% F1 (+25%)

### Nice to Have (Phase 3 - 4-8 hours):
7. ⏭️ Implement coreference resolution
8. ⏭️ Add document chunking
9. ⏭️ Add confidence-based filtering

**Expected Result**: 75% F1 (+35%)

---

## 🎯 Quick Test After Each Phase

Run evaluation after implementing each phase:

```bash
# Phase 1
export RDF_EXTRACTION_MODEL=claude-3-5-sonnet-20241022
cargo run --example docred_evaluation

# Phase 2
cargo run --example docred_evaluation

# Phase 3
cargo run --example docred_evaluation
```

Target progression:
- Baseline: **39.68% F1**
- After Phase 1: **~55% F1** ✅
- After Phase 2: **~65% F1** ✅
- After Phase 3: **~75% F1** 🎯

---

## 💡 Additional Optimizations

### Temperature Tuning
```bash
export GENAI_TEMPERATURE=0.05  # Very deterministic for documents
```

### Max Retries
```bash
export RDF_EXTRACTION_MAX_RETRIES=8  # More retries for complex documents
```

### Increase Context Window
For very long documents:
```rust
config.max_tokens = 8192;  // Use larger context window
```

---

## 📝 Testing Strategy

1. **Start with Phase 1** (model + prompt improvements)
2. **Measure improvement** on all 3 documents
3. **If still < 55% F1**, add Phase 2 relation validation
4. **If < 65% F1**, consider Phase 3 coreference resolution

**Expected Timeline**:
- Phase 1: 1-2 hours → 55% F1
- Phase 2: 2-4 hours → 65% F1
- Phase 3: 4-8 hours → 75% F1

Good luck! Start with Phase 1 and let me know the results. 🚀
