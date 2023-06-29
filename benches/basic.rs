use criterion::{criterion_group, criterion_main, Criterion};
use dawgdic::dawg::{Dawg, DawgBuilder};
use dawgdic::dictionary::DictionaryBuilder;
use std::io::BufRead;
use std::io::BufReader;
use std::path::PathBuf;

fn load_corpus_lines() -> Vec<String> {
    let corpus_file =
        std::fs::File::open(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("corpora/large.txt"))
            .unwrap();
    let reader = BufReader::new(corpus_file);
    reader.lines().map(Result::unwrap).collect()
}

#[inline(always)]
fn build_dictionary(lines: &[String]) {
    let dawg = build_dawg(lines);
    DictionaryBuilder::new(dawg).build();
}

#[inline(always)]
fn build_dawg(lines: &[String]) -> Dawg {
    lines
        .iter()
        .fold(DawgBuilder::new(), |mut builder, line| {
            builder.insert_key(line, 1);
            builder
        })
        .build()
}

fn criterion_benchmark(c: &mut Criterion) {
    let lines = load_corpus_lines();

    let mut group = c.benchmark_group("basic");
    group.sample_size(400);
    group.measurement_time(std::time::Duration::from_secs(35));
    group.noise_threshold(0.03);
    group.bench_function("build-only-dawg", |b| b.iter(|| build_dawg(&lines)));
    group.bench_function("build-dawg-and-dictionary", |b| {
        b.iter(|| build_dictionary(&lines))
    });
    group.finish();
}

criterion_group!(benches, criterion_benchmark);
criterion_main!(benches);
