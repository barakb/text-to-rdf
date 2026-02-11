//! RDF extraction implementation using genai crate

use async_trait::async_trait;
use genai::chat::{ChatMessage, ChatRequest};
use genai::Client;

use crate::chunking::{DocumentChunk, SemanticChunker};
use crate::coref::{CorefConfig, CorefResolver};
use crate::entity_linker::EntityLinker;
use crate::knowledge_buffer::KnowledgeBuffer;
use crate::{Error, ExtractionConfig, RdfDocument, RdfExtractor, Result};

/// Default system prompt for RDF extraction
const DEFAULT_SYSTEM_PROMPT: &str = r#"You are an expert RDF extraction system. Extract ONLY explicitly stated facts from text.

CRITICAL RULES:
1. Return ONLY valid JSON-LD conforming to Schema.org
2. Extract ONLY facts directly stated in the text - NO inferences or derived information
3. Use these entity types: Person, Organization, EducationalOrganization, Place, Event, Airport
4. Always include @context set to "https://schema.org/"
5. Always include @type for the main entity
6. Use @id for entity identifiers (URLs when possible)
7. Map properties to standard Schema.org properties
8. For nested entities (like birthPlace, alumniOf, location), include ONLY the name property
9. Extract dates in ISO 8601 format (YYYY-MM-DD) when explicitly mentioned
10. If extraction fails validation, you will receive specific errors and must correct them

MULTI-PARAGRAPH DOCUMENT HANDLING:
- Track entities across sentences using coreference resolution
- When you see "It", "She", "The company", "The university" - identify which entity this refers to
- Extract relations WITH CORRECT DIRECTION:
  * "Steve Jobs founded Apple" → (Steve Jobs, worksFor, Apple Inc.) NOT (Apple Inc., founder, Steve Jobs)
  * "Larry Page graduated from Stanford" → (Larry Page, alumniOf, Stanford University) NOT (Stanford, alumni, Larry Page)
  * "Apple is located in Cupertino" → (Apple Inc., location, Cupertino) NOT (Cupertino, location, Apple)
- Focus on the MAIN ENTITY (usually the document title/first entity mentioned)
- Do NOT extract properties of secondary entities unless explicitly stated

FOCUS ON CORE RELATIONS:
- Person: name, birthDate, deathDate, alumniOf, birthPlace, worksFor
- Organization: name, location, foundedBy (if founder explicitly named)
- Place: name, addressCountry, containedInPlace
- EducationalOrganization: name, location, alumniOf (reverse: Person → edu)

DO NOT EXTRACT these properties unless EXPLICITLY AND DIRECTLY stated:
- graduationDate, degree, educationalCredential (mention of year alone is NOT a graduationDate)
- founder, foundingDate (unless explicitly "founded in YYYY" or "founded by NAME")
- currentCEO, CEO (unless explicitly "current CEO" or "CEO as of DATE")
- alumni (this is reverse direction - use alumniOf on Person instead)
- gender, age, nationality
- Any property whose value must be inferred

EXAMPLES:

Input: "Alan Bean was born on March 15, 1932."
Output:
{
  "@context": "https://schema.org/",
  "@type": "Person",
  "name": "Alan Bean",
  "birthDate": "1932-03-15"
}

Input: "Alan Bean graduated from UT Austin in 1955 with a B.S."
WRONG OUTPUT (DO NOT DO THIS):
{
  "@type": "Person",
  "name": "Alan Bean",
  "alumniOf": {"@type": "EducationalOrganization", "name": "UT Austin"},
  "graduationDate": "1955",
  "degree": "B.S."
}

CORRECT OUTPUT:
{
  "@context": "https://schema.org/",
  "@type": "Person",
  "name": "Alan Bean",
  "alumniOf": {
    "@type": "EducationalOrganization",
    "name": "UT Austin"
  }
}

Input: "Apple Inc. was founded by Steve Jobs in 1976. The company is headquartered in Cupertino, California."
CORRECT OUTPUT (focus on main entity Apple Inc.):
{
  "@context": "https://schema.org/",
  "@type": "Organization",
  "name": "Apple Inc.",
  "location": {
    "@type": "Place",
    "name": "Cupertino",
    "addressCountry": "California"
  }
}

Input: "Stanford University is in California. Larry Page and Sergey Brin graduated from Stanford."
WRONG OUTPUT (extracting backwards relation):
{
  "@type": "EducationalOrganization",
  "name": "Stanford University",
  "alumni": ["Larry Page", "Sergey Brin"]
}

CORRECT OUTPUT (focus on main entity, don't extract secondary entity details):
{
  "@context": "https://schema.org/",
  "@type": "EducationalOrganization",
  "name": "Stanford University",
  "location": {
    "@type": "Place",
    "name": "California"
  }
}

Return ONLY the JSON-LD, no commentary or explanations.
"#;

/// RDF extractor implementation using the genai crate
pub struct GenAiExtractor {
    client: Client,
    config: ExtractionConfig,
    coref_resolver: CorefResolver,
    entity_linker: Option<EntityLinker>,
}

impl GenAiExtractor {
    /// Create a new GenAI-based RDF extractor
    ///
    /// # Errors
    ///
    /// Returns an error if the genai client cannot be initialized
    pub fn new(config: ExtractionConfig) -> Result<Self> {
        let client = Client::default();

        // Initialize coreference resolver from environment
        let coref_config = CorefConfig::from_env()?;
        let coref_resolver = CorefResolver::new(coref_config)?;

        // Initialize entity linker if enabled
        let entity_linker = if config.entity_linker.enabled {
            Some(EntityLinker::new(config.entity_linker.clone())?)
        } else {
            None
        };

        Ok(Self {
            client,
            config,
            coref_resolver,
            entity_linker,
        })
    }

    /// Get the system prompt for extraction
    fn get_system_prompt(&self) -> &str {
        self.config
            .system_prompt
            .as_deref()
            .unwrap_or(DEFAULT_SYSTEM_PROMPT)
    }

    /// Extract the JSON-LD content from the AI response
    fn extract_json_from_response(response: &str) -> String {
        // Try to find JSON content between code fences
        if let Some(start) = response.find("```json") {
            let after_fence = start + 7; // Skip past "```json"
            if let Some(end_offset) = response[after_fence..].find("```") {
                let json_end = after_fence + end_offset;
                return response[after_fence..json_end].trim().to_string();
            }
        }

        // Try to find raw JSON by looking for { at the start
        if let Some(start) = response.find('{') {
            if let Some(end) = response.rfind('}') {
                return response[start..=end].trim().to_string();
            }
        }

        // If no JSON found, return the whole response and let JSON parser handle it
        response.trim().to_string()
    }

    /// Generate a detailed validation error message for LLM feedback
    ///
    /// This creates a human-readable error message explaining what went wrong
    /// with the extraction, which can be sent back to the LLM for correction.
    fn generate_validation_error_message(error: &Error) -> String {
        match error {
            Error::JsonParse(e) => format!(
                "JSON Parsing Error: {e}\n\nPlease ensure:\n\
                - Valid JSON syntax (proper quotes, commas, brackets)\n\
                - No trailing commas\n\
                - Escaped special characters in strings"
            ),
            Error::Validation(msg) => format!(
                "Schema Validation Error: {msg}\n\nPlease ensure:\n\
                - @context is set to \"https://schema.org/\"\n\
                - @type is present and valid (Person, Organization, Place, Event, etc.)\n\
                - All required properties for the entity type are included\n\
                - Property names match Schema.org vocabulary"
            ),
            Error::InvalidRdf(msg) => format!(
                "RDF Structure Error: {msg}\n\nPlease ensure:\n\
                - The document follows JSON-LD structure\n\
                - All required RDF properties are present\n\
                - Nested entities have proper @type annotations"
            ),
            Error::MissingField(field) => format!(
                "Missing Required Field: {field}\n\nPlease ensure:\n\
                - All required Schema.org properties are present\n\
                - Field names are spelled correctly\n\
                - Values are not null or empty"
            ),
            _ => format!("Extraction Error: {error}\n\nPlease try again with valid JSON-LD."),
        }
    }

    /// Extract with retry logic and error feedback (Instructor pattern)
    ///
    /// This implements the Instructor pattern by:
    /// 1. Attempting extraction
    /// 2. Validating the result
    /// 3. If validation fails, sending the error back to the LLM as feedback
    /// 4. Retrying up to `max_retries` times
    async fn extract_with_retry(&self, text: &str) -> Result<RdfDocument> {
        let mut last_error = None;
        let mut conversation_history = Vec::new();

        // Initial system message
        conversation_history.push(ChatMessage::system(self.get_system_prompt()));

        for attempt in 0..=self.config.max_retries {
            // Build the user message for this attempt
            let user_message = if attempt == 0 {
                // First attempt: just the extraction request
                format!(
                    "Extract RDF entities and relations from the following text. \
                    Return only valid JSON-LD:\n\n{text}"
                )
            } else {
                // Retry attempt: include the error feedback
                let error_msg = last_error.as_ref().map_or_else(
                    || "Unknown error".to_string(),
                    Self::generate_validation_error_message,
                );

                format!(
                    "The previous extraction failed with the following error:\n\n{error_msg}\n\n\
                    Please correct the JSON-LD and extract again from this text:\n\n{text}"
                )
            };

            conversation_history.push(ChatMessage::user(user_message));

            // Execute the chat request with conversation history
            let request = ChatRequest::new(conversation_history.clone());
            let response = self
                .client
                .exec_chat(&self.config.model, request, None)
                .await
                .map_err(|e| Error::AiService(e.to_string()))?;

            // Get the response content
            let content_text = response
                .first_text()
                .ok_or_else(|| Error::AiService("Empty response from AI service".to_string()))?;

            // Add assistant response to conversation history for next iteration
            conversation_history.push(ChatMessage::assistant(content_text.to_string()));

            // Extract JSON from the response
            let json_str = Self::extract_json_from_response(content_text);

            // Try to parse and validate
            match RdfDocument::from_json(&json_str) {
                Ok(mut doc) => {
                    // Inject hardcoded context if enabled
                    if self.config.inject_hardcoded_context {
                        if let Err(e) = doc.inject_hardcoded_context() {
                            last_error = Some(e);
                            continue;
                        }
                    }

                    // If strict validation is enabled, validate the document
                    if self.config.strict_validation {
                        if let Err(e) = doc.validate() {
                            last_error = Some(e);
                            continue;
                        }
                    }
                    return Ok(doc);
                }
                Err(e) => {
                    last_error = Some(e);
                    // If we've exhausted retries, return the error
                    if attempt == self.config.max_retries {
                        return Err(last_error.unwrap());
                    }
                }
            }
        }

        // This should never be reached due to the check above, but return the last error just in case
        Err(last_error.unwrap_or_else(|| Error::Extraction("Unknown error".to_string())))
    }

    /// Estimate the number of tokens in text (rough approximation)
    const fn estimate_tokens(text: &str) -> usize {
        // Rough approximation: 1 token ≈ 4 characters for English
        text.len() / 4
    }

    /// Link entities in extracted document to canonical URIs
    async fn link_entities_in_document(&self, doc: &mut RdfDocument, text: &str) -> Result<()> {
        // Skip if no linker configured
        let Some(linker) = &self.entity_linker else {
            return Ok(());
        };

        // Extract entity names from JSON-LD
        let entity_names = Self::extract_entity_names(&doc.data);
        if entity_names.is_empty() {
            return Ok(());
        }

        // Debug logging
        if std::env::var("DEBUG_ENTITY_LINKING").is_ok() {
            println!("    🔗 Linking {} entities...", entity_names.len());
        }

        // Batch link all entities
        match linker.link_entities(text, &entity_names).await {
            Ok(linked_results) => {
                let mut linked_count = 0;
                for (name, maybe_linked) in entity_names.iter().zip(linked_results.iter()) {
                    if let Some(linked) = maybe_linked {
                        // Enrich document with URI by finding entity and setting @id
                        Self::enrich_entity_with_uri(&mut doc.data, name, &linked.uri);
                        linked_count += 1;

                        // Debug logging
                        if std::env::var("DEBUG_ENTITY_LINKING").is_ok() {
                            println!(
                                "       {} → {} (confidence: {:.2})",
                                name, linked.uri, linked.confidence
                            );
                        }
                    }
                }

                if std::env::var("DEBUG_ENTITY_LINKING").is_ok() {
                    println!(
                        "    🔗 Linked {}/{} entities successfully",
                        linked_count,
                        entity_names.len()
                    );
                }
            }
            Err(e) => {
                eprintln!("  ⚠️  Entity linking failed: {e}. Continuing without URIs.");
            }
        }

        Ok(())
    }

    /// Extract entity names from JSON-LD data
    fn extract_entity_names(value: &serde_json::Value) -> Vec<String> {
        let mut names = Vec::new();
        Self::extract_names_recursive(value, &mut names);
        names.sort();
        names.dedup();
        names
    }

    /// Recursively extract names from JSON-LD structure
    fn extract_names_recursive(value: &serde_json::Value, names: &mut Vec<String>) {
        match value {
            serde_json::Value::Object(map) => {
                if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                    names.push(name.to_string());
                }
                for v in map.values() {
                    Self::extract_names_recursive(v, names);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    Self::extract_names_recursive(v, names);
                }
            }
            _ => {}
        }
    }

    /// Enrich a specific entity with a canonical URI
    fn enrich_entity_with_uri(value: &mut serde_json::Value, entity_name: &str, uri: &str) {
        match value {
            serde_json::Value::Object(map) => {
                // Check if this entity matches the name
                if let Some(name) = map.get("name").and_then(|v| v.as_str()) {
                    if name == entity_name {
                        // Add or update @id field
                        map.insert("@id".to_string(), serde_json::json!(uri));
                        return;
                    }
                }
                // Recurse into all values
                for v in map.values_mut() {
                    Self::enrich_entity_with_uri(v, entity_name, uri);
                }
            }
            serde_json::Value::Array(arr) => {
                for v in arr {
                    Self::enrich_entity_with_uri(v, entity_name, uri);
                }
            }
            _ => {}
        }
    }

    /// Build a context-enriched prompt with knowledge buffer
    fn build_context_prompt(&self, kb: &KnowledgeBuffer) -> String {
        let base_prompt = self.get_system_prompt();

        if kb.entity_count() == 0 {
            return base_prompt.to_string();
        }

        let context_summary = kb.get_context_summary();

        format!(
            "{base_prompt}\n\n===== DOCUMENT CONTEXT =====\n{context_summary}\
            ===== END CONTEXT =====\n\n\
            Use this context to resolve pronouns and entity references in the text below."
        )
    }

    /// Extract from a single chunk with context
    async fn extract_from_chunk(
        &self,
        chunk: &DocumentChunk,
        kb: &KnowledgeBuffer,
    ) -> Result<RdfDocument> {
        // Build context-enriched prompt
        let context_prompt = self.build_context_prompt(kb);

        // Create conversation with context
        let mut conversation = vec![ChatMessage::system(context_prompt)];

        let user_message = format!(
            "Extract RDF entities and relations from the following text section. \
            Return only valid JSON-LD:\n\n{}",
            chunk.text
        );

        conversation.push(ChatMessage::user(user_message));

        // Execute the chat request
        let request = ChatRequest::new(conversation);
        let response = self
            .client
            .exec_chat(&self.config.model, request, None)
            .await
            .map_err(|e| Error::AiService(e.to_string()))?;

        // Get the response content
        let content_text = response
            .first_text()
            .ok_or_else(|| Error::AiService("Empty response from AI service".to_string()))?;

        // Extract JSON from the response
        let json_str = Self::extract_json_from_response(content_text);

        // Parse and validate
        let mut doc = RdfDocument::from_json(&json_str)?;

        // Inject hardcoded context if enabled
        if self.config.inject_hardcoded_context {
            doc.inject_hardcoded_context()?;
        }

        // Validate if strict validation is enabled
        if self.config.strict_validation {
            doc.validate()?;
        }

        Ok(doc)
    }

    /// Merge documents from multiple chunks, deduplicating entities and triples
    fn merge_chunks(docs: Vec<RdfDocument>) -> RdfDocument {
        if docs.is_empty() {
            // Return empty document with schema.org context
            return RdfDocument {
                context: serde_json::json!("https://schema.org/"),
                data: serde_json::json!({}),
                provenance: None,
            };
        }

        // Use context from first document
        let context = docs[0].context.clone();

        // Collect all entities in a @graph array
        let mut graph = Vec::new();

        for doc in docs {
            if let Some(obj) = doc.data.as_object() {
                // Check if this document has a @graph key
                if let Some(graph_array) = obj.get("@graph").and_then(|v| v.as_array()) {
                    // Add all entities from the graph
                    graph.extend(graph_array.iter().cloned());
                } else if !obj.is_empty() {
                    // This is a single entity - create a clean copy without @context
                    let mut entity = serde_json::Map::new();
                    for (key, value) in obj {
                        if key != "@context" {
                            entity.insert(key.clone(), value.clone());
                        }
                    }
                    if !entity.is_empty() {
                        graph.push(serde_json::json!(entity));
                    }
                }
            }
        }

        // Create merged document with @graph
        let merged_data = if graph.is_empty() {
            serde_json::json!({})
        } else if graph.len() == 1 {
            // If only one entity, return it directly (no @graph wrapper)
            graph.pop().unwrap()
        } else {
            // Multiple entities, use @graph
            serde_json::json!({
                "@graph": graph
            })
        };

        RdfDocument {
            context,
            data: merged_data,
            provenance: None,
        }
    }

    /// Extract from a long document using chunking and knowledge buffer
    ///
    /// This method implements document-level extraction with context preservation:
    /// 1. Checks if the document needs chunking (> 2000 tokens)
    /// 2. Splits into semantic chunks if needed
    /// 3. Tracks entities across chunks using a knowledge buffer
    /// 4. Processes chunks sequentially to maintain context
    /// 5. Merges results and deduplicates
    ///
    /// # Arguments
    /// * `text` - The full document text
    ///
    /// # Returns
    /// A merged `RdfDocument` containing all extracted entities and relations
    ///
    /// # Errors
    ///
    /// Returns an error if extraction fails
    pub async fn extract_from_document(&self, text: &str) -> Result<RdfDocument> {
        // 1. Check if document needs chunking
        let token_count = Self::estimate_tokens(text);

        // Use configurable threshold (default 2000, can be set lower for testing)
        let chunk_threshold = std::env::var("RDF_CHUNK_THRESHOLD")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .unwrap_or(2000);

        if token_count < chunk_threshold {
            // Short document - extract normally
            return self.extract(text).await;
        }

        // 2. Semantic chunking
        let chunker = SemanticChunker::default();
        let chunks = chunker.chunk(text);

        println!(
            "📊 Document is {} tokens, splitting into {} chunks",
            token_count,
            chunks.len()
        );

        // 3. Knowledge buffer for entity tracking
        let mut kb = KnowledgeBuffer::new();
        let mut all_docs = Vec::new();

        // 4. Process chunks sequentially (preserve order for coreference)
        for (idx, chunk) in chunks.iter().enumerate() {
            println!("  Processing chunk {}/{}", idx + 1, chunks.len());

            // Apply coreference resolution BEFORE extraction
            let resolved_chunk = match self.coref_resolver.resolve(&chunk.text).await {
                Ok(coref_result) => {
                    // Enrich KB with pronoun→entity mappings
                    for (pronoun, entity) in &coref_result.mention_map {
                        kb.add_alias(pronoun, entity);
                    }

                    // Debug logging
                    if std::env::var("DEBUG_COREF").is_ok() && !coref_result.mention_map.is_empty()
                    {
                        println!(
                            "    🔍 Coref: {} pronouns resolved",
                            coref_result.mention_map.len()
                        );
                    }

                    // Create chunk with resolved text, preserving original offsets
                    DocumentChunk {
                        id: chunk.id,
                        text: coref_result.resolved_text,
                        start_offset: chunk.start_offset,
                        end_offset: chunk.end_offset,
                        entities_mentioned: chunk.entities_mentioned.clone(),
                    }
                }
                Err(e) => {
                    eprintln!(
                        "  ⚠️  Coref failed for chunk {}: {}. Using original text.",
                        idx + 1,
                        e
                    );
                    chunk.clone()
                }
            };

            // Extract from resolved chunk
            match self.extract_from_chunk(&resolved_chunk, &kb).await {
                Ok(mut chunk_doc) => {
                    // Link entities before updating KB
                    if let Err(e) = self
                        .link_entities_in_document(&mut chunk_doc, &resolved_chunk.text)
                        .await
                    {
                        eprintln!("  ⚠️  Chunk {} entity linking failed: {}", idx + 1, e);
                    }

                    // Add provenance metadata if enabled
                    if self.config.provenance_tracking {
                        let provenance = crate::types::Provenance::new()
                            .with_chunk_id(idx)
                            .with_text_span(resolved_chunk.start_offset, resolved_chunk.end_offset)
                            .with_method("llm".to_string())
                            .with_source_text(resolved_chunk.text.clone());

                        chunk_doc.set_provenance(provenance);

                        if std::env::var("DEBUG_PROVENANCE").is_ok() {
                            println!(
                                "  📍 Provenance: chunk={}, span=({}, {})",
                                idx, resolved_chunk.start_offset, resolved_chunk.end_offset
                            );
                        }
                    }

                    // Update KB with discovered entities (with URIs if linked)
                    if let Some(obj) = chunk_doc.data.as_object() {
                        if let (Some(entity_type), Some(entity_name)) = (
                            obj.get("@type").and_then(|v| v.as_str()),
                            obj.get("name").and_then(|v| v.as_str()),
                        ) {
                            kb.add_entity(
                                entity_name,
                                entity_type,
                                resolved_chunk.start_offset,
                                resolved_chunk.id,
                            );

                            // Add canonical URI to KB if linked
                            if let Some(id) = obj.get("@id").and_then(|v| v.as_str()) {
                                kb.add_property(entity_name, "@id", id);
                            }
                        }
                    }
                    all_docs.push(chunk_doc);
                }
                Err(e) => {
                    eprintln!("  ⚠️  Chunk {} extraction failed: {}", idx + 1, e);
                }
            }
        }

        // 5. Merge and deduplicate
        println!("  Merging {} chunks", all_docs.len());
        Ok(Self::merge_chunks(all_docs))
    }
}

#[async_trait]
impl RdfExtractor for GenAiExtractor {
    async fn extract(&self, text: &str) -> Result<RdfDocument> {
        // Apply coreference resolution for short documents too
        let resolved_text = match self.coref_resolver.resolve(text).await {
            Ok(coref_result) => {
                if std::env::var("DEBUG_COREF").is_ok() && !coref_result.mention_map.is_empty() {
                    println!(
                        "🔍 Coref: {} pronouns resolved in short document",
                        coref_result.mention_map.len()
                    );
                }
                coref_result.resolved_text
            }
            Err(e) => {
                eprintln!("⚠️  Coref resolution failed: {e}. Using original text.");
                text.to_string()
            }
        };

        // Use the Instructor pattern with retry logic on resolved text
        let mut result = self.extract_with_retry(&resolved_text).await?;

        // Link entities to canonical URIs
        self.link_entities_in_document(&mut result, &resolved_text)
            .await?;

        // Add provenance metadata if enabled
        if self.config.provenance_tracking {
            let provenance = crate::types::Provenance::new()
                .with_text_span(0, text.len())
                .with_method("llm".to_string())
                .with_source_text(text.to_string());

            result.set_provenance(provenance);

            if std::env::var("DEBUG_PROVENANCE").is_ok() {
                println!("📍 Provenance: short document, span=(0, {})", text.len());
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extractor_creation() {
        let config = ExtractionConfig::default();
        let extractor = GenAiExtractor::new(config);
        assert!(extractor.is_ok());
    }

    #[test]
    fn test_json_extraction_from_code_fence() {
        let config = ExtractionConfig::default();
        let _extractor = GenAiExtractor::new(config).unwrap();

        let response = r#"Here's the extracted data:
```json
{"@context": "https://schema.org/", "@type": "Person"}
```
Hope this helps!"#;

        let json = GenAiExtractor::extract_json_from_response(response);
        assert!(json.contains("@context"));
    }

    #[test]
    fn test_json_extraction_raw() {
        let config = ExtractionConfig::default();
        let _extractor = GenAiExtractor::new(config).unwrap();

        let response = r#"{"@context": "https://schema.org/", "@type": "Person"}"#;

        let json = GenAiExtractor::extract_json_from_response(response);
        assert!(json.contains("@context"));
    }
}
