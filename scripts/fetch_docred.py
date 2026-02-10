#!/usr/bin/env python3
"""
Fetch real DocRED documents from Hugging Face for testing long-document extraction.

DocRED contains Wikipedia documents with ~1000 words that require cross-sentence reasoning.
"""

import json
from datasets import load_dataset

def main():
    print("📥 Downloading DocRED dataset from Hugging Face...")

    # Load validation set (smaller, fully annotated)
    dataset = load_dataset("docred", split="validation")

    print(f"✅ Loaded {len(dataset)} documents")

    # Select 3 diverse documents with multiple entities and relations
    # Filter for documents with:
    # - Multiple paragraphs (> 5 sentences)
    # - Multiple entities (> 5)
    # - Multiple relations (> 3)

    selected_docs = []

    for idx, doc in enumerate(dataset):
        sents_flat = [s for para in doc['sents'] for s in para]
        num_sentences = len(sents_flat)
        num_entities = len(doc['vertexSet'])
        num_relations = len(doc['labels'])

        # Calculate character count
        text = ' '.join(sents_flat)
        char_count = len(text)

        if num_sentences >= 10 and num_entities >= 5 and num_relations >= 3 and char_count > 2000:
            print(f"\nDocument {len(selected_docs) + 1}:")
            print(f"  Title: {doc['title']}")
            print(f"  Sentences: {num_sentences}")
            print(f"  Characters: {char_count}")
            print(f"  Entities: {num_entities}")
            print(f"  Relations: {num_relations}")

            selected_docs.append({
                'id': f"docred_{idx}",
                'title': doc['title'],
                'sents': doc['sents'],
                'vertexSet': doc['vertexSet'],
                'labels': doc['labels']
            })

            if len(selected_docs) >= 3:
                break

    # Save to JSON
    output_path = 'tests/fixtures/docred_long_docs.json'
    with open(output_path, 'w') as f:
        json.dump(selected_docs, f, indent=2, ensure_ascii=False)

    print(f"\n✅ Saved {len(selected_docs)} documents to {output_path}")

    # Print summary
    print("\n" + "="*60)
    print("SUMMARY")
    print("="*60)
    for doc in selected_docs:
        sents_flat = [s for para in doc['sents'] for s in para]
        text = ' '.join(sents_flat)
        tokens = len(text) // 4  # Rough estimate

        print(f"\n{doc['title']}:")
        print(f"  ~{tokens} tokens ({len(text)} chars)")
        print(f"  Will trigger chunking: {'YES ✅' if tokens > 2000 else 'NO ❌'}")

if __name__ == '__main__':
    main()
