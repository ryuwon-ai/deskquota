//! Isolated tokenizer comparison (the upstream oracle is a dev dependency).
//! bench_tokenizer <compact|upstream> <cl100k|o200k> <utf8-file> [iterations] [hold-ms]
use llmgw::input_estimate::InputEstimator;
use std::{hint::black_box, time::Instant};

fn main() {
    let args: Vec<_> = std::env::args().collect();
    assert!(
        (4..=6).contains(&args.len()),
        "usage: bench_tokenizer <compact|upstream> <cl100k|o200k> <utf8-file> [iterations] [hold-ms]"
    );
    let text = std::fs::read_to_string(&args[3]).expect("UTF-8 input file");
    let iterations: usize = args
        .get(4)
        .map(|s| s.parse().expect("iterations"))
        .unwrap_or(100);
    assert!(iterations > 0);
    let estimator = match args[2].as_str() {
        "cl100k" => InputEstimator::Cl100kBase,
        "o200k" => InputEstimator::O200kBase,
        _ => panic!("unknown encoding"),
    };
    let start = Instant::now();
    let count: Box<dyn Fn(&str) -> usize> = match args[1].as_str() {
        "compact" => {
            estimator.estimate("", 0).expect("BPE enabled");
            Box::new(move |text| estimator.estimate(text, 0).expect("count") as usize)
        }
        "upstream" => {
            let tokenizer = match estimator {
                InputEstimator::Cl100kBase => bpe_openai::cl100k_base(),
                _ => bpe_openai::o200k_base(),
            };
            Box::new(move |text| tokenizer.count(text))
        }
        _ => panic!("unknown implementation"),
    };
    let init_us = start.elapsed().as_micros();
    let start = Instant::now();
    let tokens = count(black_box(&text));
    let first_count_us = start.elapsed().as_micros();
    let mut samples = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        assert_eq!(black_box(count(black_box(&text))), tokens);
        samples.push(start.elapsed().as_micros());
    }
    samples.sort_unstable();
    println!(
        "{}",
        serde_json::json!({
            "implementation": args[1], "encoding": args[2], "pid": std::process::id(),
            "input_bytes": text.len(), "tokens": tokens, "iterations": iterations,
            "init_us": init_us, "first_count_us": first_count_us,
            "warm_median_us": samples[iterations / 2],
            "warm_p95_us": samples[(iterations * 95).div_ceil(100) - 1],
        })
    );
    if let Some(hold) = args.get(5) {
        std::thread::sleep(std::time::Duration::from_millis(
            hold.parse().expect("hold-ms"),
        ));
    }
}
