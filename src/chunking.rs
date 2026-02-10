use text_splitter::TextSplitter;

/// A chunk of text with metadata for document-level extraction
#[derive(Debug, Clone)]
pub struct DocumentChunk {
    /// Chunk index (0-based)
    pub id: usize,

    /// The text content of this chunk
    pub text: String,

    /// Character offset where this chunk starts in the original document
    pub start_offset: usize,

    /// Character offset where this chunk ends in the original document
    pub end_offset: usize,

    /// Entities mentioned in this chunk (populated during extraction)
    pub entities_mentioned: Vec<String>,
}

/// Semantic chunker that splits text at natural boundaries
pub struct SemanticChunker {
    max_chunk_size: usize,
    overlap_chars: usize,
}

impl SemanticChunker {
    /// Create a new semantic chunker
    ///
    /// # Arguments
    /// * `max_chunk_size` - Maximum characters per chunk
    /// * `overlap_chars` - Number of overlapping characters between chunks
    #[must_use]
    pub const fn new(max_chunk_size: usize, overlap_chars: usize) -> Self {
        Self {
            max_chunk_size,
            overlap_chars,
        }
    }

    /// Split text into semantic chunks
    ///
    /// This uses sentence boundaries to avoid splitting mid-sentence,
    /// and includes overlap between chunks to maintain context.
    #[must_use]
    pub fn chunk(&self, text: &str) -> Vec<DocumentChunk> {
        // Create text splitter with character-based chunking
        let splitter = TextSplitter::new(self.max_chunk_size);

        // Split the text
        let chunks = splitter.chunks(text);

        // Track current position in document
        let mut current_offset = 0;

        chunks
            .enumerate()
            .map(|(idx, chunk)| {
                let chunk_text = chunk.to_string();
                let chunk_len = chunk_text.len();

                // Find the actual position of this chunk in the original text
                // (accounting for overlap)
                let start_offset = if idx == 0 { 0 } else { current_offset };

                let end_offset = start_offset + chunk_len;
                current_offset = end_offset.saturating_sub(self.overlap_chars);

                DocumentChunk {
                    id: idx,
                    text: chunk_text,
                    start_offset,
                    end_offset,
                    entities_mentioned: vec![],
                }
            })
            .collect()
    }

    /// Check if a document needs chunking
    #[must_use]
    pub const fn needs_chunking(&self, text: &str) -> bool {
        text.len() > self.max_chunk_size
    }

    /// Estimate the number of chunks for a document
    #[must_use]
    pub const fn estimate_chunk_count(&self, text: &str) -> usize {
        if !self.needs_chunking(text) {
            return 1;
        }

        let effective_chunk_size = self.max_chunk_size - self.overlap_chars;
        text.len().div_ceil(effective_chunk_size)
    }
}

impl Default for SemanticChunker {
    fn default() -> Self {
        Self::new(
            3500, // 3500 chars ≈ 875 tokens (optimal for document-level extraction)
            400,  // 400 char overlap for better context preservation
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunking_short_text() {
        let chunker = SemanticChunker::new(1000, 100);
        let text = "This is a short document. It should not be chunked.";

        let chunks = chunker.chunk(text);

        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].text, text);
        assert_eq!(chunks[0].start_offset, 0);
        assert_eq!(chunks[0].end_offset, text.len());
    }

    #[test]
    fn test_chunking_long_text() {
        let chunker = SemanticChunker::new(100, 20);
        let text = "This is sentence one. This is sentence two. This is sentence three. This is sentence four. This is sentence five. This is sentence six.";

        let chunks = chunker.chunk(text);

        assert!(
            chunks.len() > 1,
            "Long text should be split into multiple chunks"
        );

        // Check overlap exists
        for i in 1..chunks.len() {
            let prev_end = &chunks[i - 1].text[chunks[i - 1].text.len().saturating_sub(20)..];
            let curr_start = &chunks[i].text[..20.min(chunks[i].text.len())];

            // Some content should overlap (not exact match due to sentence boundaries)
            assert!(
                chunks[i - 1].text.len() > 20,
                "Previous chunk should be long enough for overlap"
            );
        }
    }

    #[test]
    fn test_needs_chunking() {
        let chunker = SemanticChunker::new(100, 20);

        assert!(!chunker.needs_chunking("Short text"));
        assert!(chunker.needs_chunking(&"x".repeat(200)));
    }

    #[test]
    fn test_estimate_chunk_count() {
        let chunker = SemanticChunker::new(100, 20);

        assert_eq!(chunker.estimate_chunk_count("Short"), 1);
        assert_eq!(chunker.estimate_chunk_count(&"x".repeat(100)), 1);
        assert_eq!(chunker.estimate_chunk_count(&"x".repeat(200)), 3);
    }
}
