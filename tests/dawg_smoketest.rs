use dawgdic::dawg::DawgBuilder;
use dawgdic::dictionary::{Dictionary, DictionaryBuilder};
use std::io::{BufRead, BufWriter, Cursor};
use std::path::PathBuf;

#[test]
fn creates_correct_dawg_shape() {
    let corpus = load_test_corpus();

    let dawg = corpus
        .into_iter()
        .fold(DawgBuilder::new(), |mut builder, (key, value)| {
            builder.insert_key(&key, value);
            builder
        })
        .build();

    assert_eq!(dawg.states_count(), 3085);
    assert_eq!(dawg.transition_count(), 4082);
    assert_eq!(dawg.merged_states_count(), 998);
    assert_eq!(dawg.merging_states_count(), 0);
    assert_eq!(dawg.merged_transitions_count(), 0);
}

#[test]
fn creates_correct_dictionary() {
    let corpus = load_test_corpus();

    let dawg = corpus
        .iter()
        .fold(DawgBuilder::new(), |mut builder, (key, value)| {
            builder.insert_key(key, *value);
            builder
        })
        .build();

    let dictionary = DictionaryBuilder::new(dawg).build();

    // Quickly checking a couple cases

    assert_eq!(dictionary.contains("this".as_bytes()), true);
    assert_eq!(dictionary.contains("loremaster".as_bytes()), false);

    assert_eq!(dictionary.find("act".as_bytes()), Some(510473));
    assert_eq!(dictionary.find("annulment".as_bytes()), None);

    // Complete check – iterate over the corpus asserting all values are packed correctly

    corpus.into_iter().for_each(|(key, value)| {
        assert_eq!(dictionary.find(key.as_bytes()), Some(value));
    })
}

#[test]
fn serializes_and_deserializes_dictionary() {
    let corpus = load_test_corpus();

    let dawg = corpus
        .iter()
        .fold(DawgBuilder::new(), |mut builder, (key, value)| {
            builder.insert_key(key, *value);
            builder
        })
        .build();

    let dictionary = DictionaryBuilder::new(dawg).build();

    let mut data_buf: Vec<u8> = Vec::new();
    dictionary.write(&mut BufWriter::new(&mut data_buf));

    assert_eq!(data_buf.len(), 17412);

    let new_dictionary = Dictionary::from_reader(&mut Cursor::new(data_buf)).unwrap();

    assert_eq!(dictionary.size(), new_dictionary.size());

    // Check new dictionary is functional in the same way

    corpus.into_iter().for_each(|(key, value)| {
        assert_eq!(dictionary.find(key.as_bytes()), Some(value));
    })
}

fn load_test_corpus() -> Vec<(String, u32)> {
    let corpus_file_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpora/01_dawg_smoketest.txt");
    std::fs::read_to_string(corpus_file_path)
        .map(|corpus_data| {
            corpus_data
                .lines()
                .map(|line| {
                    let components = line.split('\t').collect::<Vec<&str>>();
                    let key = components[0].to_string();
                    let value = components[1].parse::<u32>().unwrap();
                    (key, value)
                })
                .collect()
        })
        .expect("Failed to parse corpus")
}
