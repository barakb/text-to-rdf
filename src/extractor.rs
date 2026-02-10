//! RDF extraction implementation using genai crate

use async_trait::async_trait;
use genai::chat::{ChatMessage, ChatRequest};
use genai::Client;

use crate::chunking::{DocumentChunk, SemanticChunker};
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
}

impl GenAiExtractor {
    /// Create a new GenAI-based RDF extractor
    ///
    /// # Errors
    ///
    /// Returns an error if the genai client cannot be initialized
    pub fn new(config: ExtractionConfig) -> Result<Self> {
        let client = Client::default();

        Ok(Self { client, config })
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
    fn estimate_tokens(&self, text: &str) -> usize {
        // Rough approximation: 1 token ≈ 4 characters for English
        text.len() / 4
    }

    /// Build a context-enriched prompt with knowledge buffer
    fn build_context_prompt(&self, kb: &KnowledgeBuffer) -> String {
        let base_prompt = self.get_system_prompt();

        if kb.entity_count() == 0 {
            return base_prompt.to_string();
        }

        let context_summary = kb.get_context_summary();

        format!(
            "{}\n\n===== DOCUMENT CONTEXT =====\n{}\
            ===== END CONTEXT =====\n\n\
            Use this context to resolve pronouns and entity references in the text below.",
            base_prompt, context_summary
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
    fn merge_chunks(&self, docs: Vec<RdfDocument>) -> RdfDocument {
        if docs.is_empty() {
            // Return empty document with schema.org context
            return RdfDocument {
                context: serde_json::json!("https://schema.org/"),
                data: serde_json::json!({}),
            };
        }

        // Use context from first document
        let context = docs[0].context.clone();

        let mut merged_data = serde_json::json!({});

        // Collect all data from chunks
        for doc in docs {
            if let Some(obj) = doc.data.as_object() {
                if let Some(merged_obj) = merged_data.as_object_mut() {
                    for (key, value) in obj {
                        // Skip @context since we handle it separately
                        if key == "@context" {
                            continue;
                        }

                        // For primitive values, keep first occurrence
                        // For arrays and objects, we could implement smarter merging
                        if !merged_obj.contains_key(key) {
                            merged_obj.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
        }

        RdfDocument {
            context,
            data: merged_data,
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
    /// A merged RdfDocument containing all extracted entities and relations
    ///
    /// # Errors
    ///
    /// Returns an error if extraction fails
    pub async fn extract_from_document(&self, text: &str) -> Result<RdfDocument> {
        // 1. Check if document needs chunking
        let token_count = self.estimate_tokens(text);

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

            match self.extract_from_chunk(chunk, &kb).await {
                Ok(chunk_doc) => {
                    // Update knowledge buffer with discovered entities from JSON data
                    if let Some(obj) = chunk_doc.data.as_object() {
                        // Extract entity type and name from the data
                        if let (Some(entity_type), Some(entity_name)) = (
                            obj.get("@type").and_then(|v| v.as_str()),
                            obj.get("name").and_then(|v| v.as_str()),
                        ) {
                            kb.add_entity(entity_name, entity_type, chunk.start_offset, chunk.id);
                        }
                    }

                    all_docs.push(chunk_doc);
                }
                Err(e) => {
                    eprintln!("  ⚠️  Chunk {} extraction failed: {}", idx + 1, e);
                    // Continue processing other chunks
                }
            }
        }

        // 5. Merge and deduplicate
        println!("  Merging {} chunks", all_docs.len());
        Ok(self.merge_chunks(all_docs))
    }
}

#[async_trait]
impl RdfExtractor for GenAiExtractor {
    async fn extract(&self, text: &str) -> Result<RdfDocument> {
        // Use the Instructor pattern with retry logic
        self.extract_with_retry(text).await
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
