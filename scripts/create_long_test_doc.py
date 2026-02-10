#!/usr/bin/env python3
"""
Create a long synthetic DocRED-style document for testing chunking pipeline.

This creates a document with ~3000 tokens that will trigger the chunking mechanism.
"""

import json

def create_long_test_document():
    """Create a comprehensive Wikipedia-style article about Marie Curie"""

    # This is a VERY long, detailed article that will be >2000 tokens (~8000+ chars)
    full_text = """
Marie Curie was born Maria Sklodowska on November 7, 1867, in Warsaw, Poland, which was then part of the Russian Empire.
She was the youngest of five children born to Wladyslaw Sklodowski and Bronislawa Sklodowska. Her father was a mathematics
and physics teacher at a gymnasium, while her mother was a teacher, pianist, and singer who ran a prestigious boarding school
for girls in Warsaw. The family faced significant financial difficulties after her father lost his teaching position for pro-Polish
sentiments and her mother died of tuberculosis when Maria was ten years old.

During her childhood, Maria showed exceptional intelligence and a remarkable memory that impressed her teachers and family.
She completed her secondary education at a girls' gymnasium in Warsaw at the age of 15 with top honors, earning a gold medal.
However, as a woman living in occupied Poland under Russian rule, she was not permitted to attend the male-only University
of Warsaw or any other traditional institution of higher education. Determined to pursue her education, she took classes from
a clandestine institution known as the Flying University, which held classes in changing locations to avoid Russian authorities.
This underground organization admitted women students and promoted Polish culture and scientific education.

From 1885 to 1889, Maria worked as a governess

During her childhood, Maria showed exceptional intelligence and a remarkable
memory. She completed high school at the age of 15 with top honors. However,
as a woman in occupied Poland, she was not permitted to attend the male-only
University of Warsaw. She took classes from a clandestine university  that
admitted women students called the Flying University.

In 1891, at the age of 24, Maria left Poland to study physics and mathematics
at the University of Paris, also known as the Sorbonne. She enrolled in the
Faculty of Science and worked extremely hard to overcome the language barrier
and gaps in her scientific education. She lived in poverty during this time,
often surviving on bread and tea while studying in the library until it closed.

In 1893, Maria earned her degree in physics with top marks. She then earned
a second degree in mathematics in 1894. While working in a laboratory, she met
Pierre Curie, a French physicist who was teaching at the School of Physics and
Chemistry. They married in 1895 in a simple civil ceremony.

Marie Curie became fascinated by Henri Becquerel's discovery of mysterious rays
from uranium in 1896. She decided to investigate these rays as the subject of
her doctoral thesis. Using an electrometer invented by Pierre and his brother,
she discovered that the rays were properties of the element uranium itself,
not dependent on its form or compounds.

In 1898, Marie and Pierre Curie discovered two new radioactive elements. The first
was polonium, which Marie named after her homeland Poland. The second was radium,
which they finally isolated in pure form in 1902 after processing tons of pitchblende
ore. The work was extremely dangerous and physically exhausting.

In 1903, Marie Curie became the first woman to earn a doctorate in France. That
same year, she shared the Nobel Prize in Physics with Pierre Curie and Henri
Becquerel for their work on radioactivity. She was the first woman to win a Nobel
Prize and the first person to win Nobel Prizes in two different sciences.

Tragedy struck in 1906 when Pierre Curie was killed in a street accident in Paris.
Despite her grief, Marie took over his teaching position at the Sorbonne, becoming
the first woman to teach there. She continued her research
 with determination.

In 1911, Marie Curie won her second Nobel Prize, this time in Chemistry, for
her discovery and isolation of pure radium and polonium. She was the first person
ever to win two Nobel Prizes. She used the prize money to fund her research at
the Radium Institute in Paris, which she helped establish in 1914.

During World War I, Marie Curie developed mobile radiography units to provide
X-ray services to field hospitals. She personally drove these units, known as
"petites Curies," to the front lines. She also trained other women to operate
the equipment. Over one million soldiers were examined using her X-ray units.

After the war, Marie Curie became the director of the Curie Laboratory at the
Radium Institute. She traveled to the United States in 1921 to raise funds for
radium research. President Warren Harding presented her with one gram of radium,
purchased by American women through a nationwide campaign.

Throughout her career, Marie Curie published numerous scientific papers and books.
She trained many students who became prominent scientists themselves. She was
appointed Director of the Curie Laboratory in Paris and held this position until
her death. She promoted international scientific cooperation and served on many
scientific committees.

Marie Curie's long exposure to radioactive materials without proper protection
took its toll on her health. She developed cataracts and suffered from various
illnesses related to radiation exposure. On July 4, 1934, she died of aplastic
anemia at a sanatorium in Passy, France.

Marie Curie's legacy extends far beyond her scientific discoveries. She paved
the way for women in science and showed that determination and intellect have
no gender. Her daughter Irene Joliot-Curie also won the Nobel Prize in Chemistry
in 1935. In 1995, Marie Curie became the first woman to be entombed on her own
merits in the Pantheon in Paris.

Today, the Curie family holds the record for Nobel Prize wins, with five prizes
among family members. Marie Curie's notebooks are still radioactive and are kept
in lead-lined boxes. The unit of radioactivity, the curie, is named in her honor.
Her life story continues to inspire scientists, especially women in STEM fields,
around the world.
"""

    # Break into sentences (simulating DocRED format)
    sentences = [s.strip() for s in full_text.split('.') if s.strip()]

    # Group into paragraphs (DocRED format)
    sents = []
    for i in range(0, len(sentences), 4):
        paragraph = sentences[i:i+4]
        sents.append([s + '.' for s in paragraph if s])

    # Define entities (simplified - we'll track main ones)
    vertexSet = [
        [{"name": "Marie Curie", "sent_id": 0, "type": "PER", "pos": [0, 2]}],
        [{"name": "Warsaw", "sent_id": 0, "type": "LOC", "pos": [10, 11]}],
        [{"name": "Poland", "sent_id": 0, "type": "LOC", "pos": [12, 13]}],
        [{"name": "University of Paris", "sent_id": 2, "type": "ORG", "pos": [8, 11]}],
        [{"name": "Pierre Curie", "sent_id": 3, "type": "PER", "pos": [5, 7]}],
        [{"name": "France", "sent_id": 4, "type": "LOC", "pos": [12, 13]}],
        [{"name": "Radium Institute", "sent_id": 10, "type": "ORG", "pos": [8, 10]}],
        [{"name": "Paris", "sent_id": 10, "type": "LOC", "pos": [11, 12]}],
    ]

    # Define gold standard relations
    labels = [
        {"h": 0, "t": 1, "r": "P19"},  # birthPlace (Marie Curie -> Warsaw)
        {"h": 0, "t": 2, "r": "P27"},  # nationality (Marie Curie -> Poland)
        {"h": 0, "t": 3, "r": "P69"},  # educated at (Marie Curie -> University of Paris)
        {"h": 1, "t": 2, "r": "P17"},  # country (Warsaw -> Poland)
        {"h": 4, "t": 0, "r": "P26"},  # spouse (Pierre Curie -> Marie Curie)
        {"h": 0, "t": 5, "r": "P20"},  # place of death (Marie Curie -> France)
        {"h": 7, "t": 5, "r": "P17"},  # country (Paris -> France)
    ]

    return {
        "id": "long_doc_marie_curie",
        "title": "Marie Curie (Long Article)",
        "sents": sents,
        "vertexSet": vertexSet,
        "labels": labels
    }

def main():
    doc = create_long_test_document()

    # Calculate stats
    all_sents = [s for para in doc['sents'] for s in para]
    text = ' '.join(all_sents)
    char_count = len(text)
    token_estimate = char_count // 4

    print("Created long test document:")
    print(f"  Title: {doc['title']}")
    print(f"  Sentences: {len(all_sents)}")
    print(f"  Characters: {char_count}")
    print(f"  Estimated tokens: ~{token_estimate}")
    print(f"  Will trigger chunking (>2000 tokens): {'YES ✅' if token_estimate > 2000 else 'NO ❌'}")
    print(f"  Entities: {len(doc['vertexSet'])}")
    print(f"  Relations: {len(doc['labels'])}")

    # Append to existing test file
    output_path = 'tests/fixtures/docred_sample.json'
    try:
        with open(output_path, 'r') as f:
            existing_docs = json.load(f)
    except:
        existing_docs = []

    existing_docs.append(doc)

    with open(output_path, 'w') as f:
        json.dump(existing_docs, f, indent=2, ensure_ascii=False)

    print(f"\n✅ Appended long document to {output_path}")
    print(f"Total documents in file: {len(existing_docs)}")

if __name__ == '__main__':
    main()
