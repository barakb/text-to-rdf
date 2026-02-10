use std::collections::HashMap;

/// Tracks entities discovered across document chunks to maintain context
pub struct KnowledgeBuffer {
    entities: HashMap<String, EntityContext>,
}

/// Context information about an entity discovered in the document
#[derive(Debug, Clone)]
pub struct EntityContext {
    /// Canonical name of the entity
    pub canonical_name: String,

    /// Entity type (Person, Organization, Place, etc.)
    pub entity_type: String,

    /// Character offset where entity was first mentioned
    pub first_mention_offset: usize,

    /// Alternative names/aliases for this entity
    pub aliases: Vec<String>,

    /// Properties discovered about this entity
    pub properties: HashMap<String, String>,

    /// Chunk ID where entity was first discovered
    pub first_chunk_id: usize,
}

impl KnowledgeBuffer {
    /// Create a new empty knowledge buffer
    #[must_use]
    pub fn new() -> Self {
        Self {
            entities: HashMap::new(),
        }
    }

    /// Add an entity discovered in a chunk
    pub fn add_entity(&mut self, name: &str, entity_type: &str, offset: usize, chunk_id: usize) {
        self.entities
            .entry(name.to_lowercase())
            .or_insert_with(|| EntityContext {
                canonical_name: name.to_string(),
                entity_type: entity_type.to_string(),
                first_mention_offset: offset,
                aliases: vec![],
                properties: HashMap::new(),
                first_chunk_id: chunk_id,
            });
    }

    /// Register an alias for an entity (e.g., "the company" → "Apple Inc.")
    pub fn add_alias(&mut self, alias: &str, canonical: &str) {
        let canonical_key = canonical.to_lowercase();
        if let Some(entity) = self.entities.get_mut(&canonical_key) {
            let alias_lower = alias.to_lowercase();

            if !entity.aliases.contains(&alias_lower) {
                entity.aliases.push(alias_lower);
            }
        }
    }

    /// Add a property to an entity
    pub fn add_property(&mut self, entity_name: &str, property: &str, value: &str) {
        let entity_key = entity_name.to_lowercase();
        if let Some(entity) = self.entities.get_mut(&entity_key) {
            entity
                .properties
                .insert(property.to_string(), value.to_string());
        }
    }

    /// Get a formatted summary of entities for prompt injection
    #[must_use]
    pub fn get_context_summary(&self) -> String {
        use std::fmt::Write;

        if self.entities.is_empty() {
            return String::new();
        }

        let mut summary = String::from("ENTITIES ALREADY DISCOVERED IN THIS DOCUMENT:\n");

        for ctx in self.entities.values() {
            write!(summary, "- {} ({})", ctx.canonical_name, ctx.entity_type).ok();

            if !ctx.aliases.is_empty() {
                write!(summary, " [also called: {}]", ctx.aliases.join(", ")).ok();
            }

            if !ctx.properties.is_empty() {
                let props: Vec<String> = ctx
                    .properties
                    .iter()
                    .map(|(k, v)| format!("{k}: {v}"))
                    .collect();
                write!(summary, " [{}]", props.join(", ")).ok();
            }

            summary.push('\n');
        }

        summary
    }

    /// Resolve an alias to canonical name
    #[must_use]
    pub fn resolve_alias(&self, text: &str) -> Option<String> {
        let text_lower = text.to_lowercase();

        for ctx in self.entities.values() {
            if ctx.aliases.iter().any(|a| a == &text_lower) {
                return Some(ctx.canonical_name.clone());
            }
        }

        None
    }

    /// Get the most recent entity of a specific type
    #[must_use]
    pub fn get_last_entity_of_type(&self, entity_type: &str) -> Option<String> {
        self.entities
            .values()
            .filter(|ctx| ctx.entity_type == entity_type)
            .max_by_key(|ctx| ctx.first_mention_offset)
            .map(|ctx| ctx.canonical_name.clone())
    }

    /// Check if an entity has been discovered
    #[must_use]
    pub fn has_entity(&self, name: &str) -> bool {
        self.entities.contains_key(&name.to_lowercase())
    }

    /// Get all entities of a specific type
    #[must_use]
    pub fn get_entities_of_type(&self, entity_type: &str) -> Vec<&EntityContext> {
        self.entities
            .values()
            .filter(|ctx| ctx.entity_type == entity_type)
            .collect()
    }

    /// Get total number of entities tracked
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    /// Get an entity context by name
    #[must_use]
    pub fn get_entity(&self, name: &str) -> Option<&EntityContext> {
        self.entities.get(&name.to_lowercase())
    }

    /// Clear all tracked entities (for starting a new document)
    pub fn clear(&mut self) {
        self.entities.clear();
    }
}

impl Default for KnowledgeBuffer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_add_entity() {
        let mut kb = KnowledgeBuffer::new();

        kb.add_entity("Marie Curie", "Person", 0, 0);

        assert!(kb.has_entity("Marie Curie"));
        assert!(kb.has_entity("marie curie")); // Case insensitive
        assert_eq!(kb.entity_count(), 1);
    }

    #[test]
    fn test_add_alias() {
        let mut kb = KnowledgeBuffer::new();

        kb.add_entity("Apple Inc.", "Organization", 0, 0);
        kb.add_alias("the company", "Apple Inc.");
        kb.add_alias("AAPL", "Apple Inc.");

        assert_eq!(
            kb.resolve_alias("the company"),
            Some("Apple Inc.".to_string())
        );
        assert_eq!(kb.resolve_alias("AAPL"), Some("Apple Inc.".to_string()));
        assert_eq!(kb.resolve_alias("unknown"), None);
    }

    #[test]
    fn test_add_property() {
        let mut kb = KnowledgeBuffer::new();

        kb.add_entity("Apple Inc.", "Organization", 0, 0);
        kb.add_property("Apple Inc.", "foundedYear", "1976");
        kb.add_property("Apple Inc.", "location", "Cupertino");

        let entity = kb.get_entity("Apple Inc.").unwrap();
        assert_eq!(
            entity.properties.get("foundedYear"),
            Some(&"1976".to_string())
        );
        assert_eq!(
            entity.properties.get("location"),
            Some(&"Cupertino".to_string())
        );
    }

    #[test]
    fn test_get_context_summary() {
        let mut kb = KnowledgeBuffer::new();

        kb.add_entity("Marie Curie", "Person", 0, 0);
        kb.add_alias("she", "Marie Curie");
        kb.add_property("Marie Curie", "birthPlace", "Warsaw");

        kb.add_entity("University of Paris", "Organization", 100, 0);

        let summary = kb.get_context_summary();

        assert!(summary.contains("Marie Curie"));
        assert!(summary.contains("Person"));
        assert!(summary.contains("University of Paris"));
        assert!(summary.contains("Organization"));
    }

    #[test]
    fn test_get_last_entity_of_type() {
        let mut kb = KnowledgeBuffer::new();

        kb.add_entity("Person A", "Person", 0, 0);
        kb.add_entity("Person B", "Person", 100, 0);
        kb.add_entity("Org A", "Organization", 50, 0);

        assert_eq!(
            kb.get_last_entity_of_type("Person"),
            Some("Person B".to_string())
        );
        assert_eq!(
            kb.get_last_entity_of_type("Organization"),
            Some("Org A".to_string())
        );
        assert_eq!(kb.get_last_entity_of_type("Place"), None);
    }

    #[test]
    fn test_get_entities_of_type() {
        let mut kb = KnowledgeBuffer::new();

        kb.add_entity("Person A", "Person", 0, 0);
        kb.add_entity("Person B", "Person", 100, 0);
        kb.add_entity("Org A", "Organization", 50, 0);

        let people = kb.get_entities_of_type("Person");
        assert_eq!(people.len(), 2);

        let orgs = kb.get_entities_of_type("Organization");
        assert_eq!(orgs.len(), 1);
    }

    #[test]
    fn test_clear() {
        let mut kb = KnowledgeBuffer::new();

        kb.add_entity("Entity A", "Person", 0, 0);
        kb.add_entity("Entity B", "Organization", 0, 0);

        assert_eq!(kb.entity_count(), 2);

        kb.clear();

        assert_eq!(kb.entity_count(), 0);
        assert!(!kb.has_entity("Entity A"));
    }
}
