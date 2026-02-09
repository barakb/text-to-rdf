//! RDF extraction implementation using genai crate

use async_trait::async_trait;
use genai::chat::{ChatMessage, ChatRequest};
use genai::Client;

use crate::{Error, ExtractionConfig, RdfDocument, RdfExtractor, Result};

/// Default system prompt for RDF extraction
const DEFAULT_SYSTEM_PROMPT: &str = r#"You are an expert RDF extraction system. Your task is to analyze text and extract structured entities and relationships in JSON-LD format.

IMPORTANT INSTRUCTIONS:
1. Return ONLY valid JSON-LD that conforms to Schema.org vocabulary
2. Use these core entity types: Person, Organization, EducationalOrganization, Place, Event
3. Always include @context set to "https://schema.org/"
4. Always include @type for the main entity
5. Use @id for entity identifiers (URLs when possible)
6. Map all properties to standard Schema.org properties
7. For nested entities (like birthPlace), include full entity structure with @type
8. Extract dates in ISO 8601 format when possible
9. Do not add commentary or explanations, only return the JSON-LD
10. If extraction fails validation, you will receive the specific errors and must correct them

Example output format:
{
  "@context": "https://schema.org/",
  "@type": "Person",
  "@id": "https://example.org/person/john-doe",
  "name": "John Doe",
  "birthDate": "1980-01-01",
  "birthPlace": {
    "@type": "Place",
    "name": "New York"
  }
}
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
