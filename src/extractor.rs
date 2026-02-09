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

    /// Build a chat request for extraction
    fn build_request(&self, text: &str) -> ChatRequest {
        let system_msg = ChatMessage::system(self.get_system_prompt());
        let user_msg = ChatMessage::user(format!(
            "Extract RDF entities and relations from the following text. Return only valid JSON-LD:\n\n{text}"
        ));

        ChatRequest::new(vec![system_msg, user_msg])
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
}

#[async_trait]
impl RdfExtractor for GenAiExtractor {
    async fn extract(&self, text: &str) -> Result<RdfDocument> {
        let request = self.build_request(text);

        // Execute the chat request
        let response = self
            .client
            .exec_chat(&self.config.model, request, None)
            .await
            .map_err(|e| Error::AiService(e.to_string()))?;

        // Get the response content text using the new genai 0.5 API
        let content_text = response
            .first_text()
            .ok_or_else(|| Error::AiService("Empty response from AI service".to_string()))?;

        // Extract JSON from the response
        let json_str = Self::extract_json_from_response(content_text);

        // Parse as RDF document
        RdfDocument::from_json(&json_str)
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
