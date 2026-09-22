#![cfg(feature = "bpe")]

use llmgw::input_estimate::InputEstimator;

#[test]
fn multilingual_and_adversarial_counts_match_pinned_upstream() {
    let mut cases: Vec<String> = [
        "",
        "hello world",
        "안녕하세요 세계",
        "日本語と中文测试",
        "العربية हिन्दी ไทย",
        "e\u{301} é 가 \u{1100}\u{1161}",
        "👩🏽‍💻👨‍👩‍👧‍👦🇰🇷🧑‍🚀",
        "I'm WE'RE isn't they'd",
        " a\r\n\t  b  \n   ",
        "\u{00a0}\u{2003}\u{2028}a\u{2029}\u{3000} ",
        "\0\u{1}\u{7}\u{1f}\u{7f}\u{85}",
        "a1234567890+/\\_你好😀\r\n",
        "<|endoftext|><|im_start|><|fim_suffix|>",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    for count in [1, 2, 3, 64, 127, 128, 129, 4096, 65536] {
        for character in [" ", "a", "\n", "\u{2003}", "한", "!", "0"] {
            cases.push(character.repeat(count));
        }
    }
    cases.push("fn 예제(a: &str) { println!(\"{} 日本語 العربية 😀\", a); }\n".repeat(8000));
    let alphabet: Vec<char> = "abcXYZ0123 '\r\n\t!@/\\_é\u{301}\u{0}\u{85}\u{a0}\u{2003}\u{2028}가나다日本中文العربيةहिन्दीไทย👩🏽‍💻".chars().collect();
    let mut seed = 0x8e12_f34a_71b9_02cdu64;
    for length in 0..2048 {
        let mut text = String::new();
        for _ in 0..length % 257 {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            text.push(alphabet[seed as usize % alphabet.len()]);
        }
        cases.push(text);
    }
    for (candidate, oracle) in [
        (InputEstimator::Cl100kBase, bpe_openai::cl100k_base()),
        (InputEstimator::O200kBase, bpe_openai::o200k_base()),
    ] {
        for (index, text) in cases.iter().enumerate() {
            assert_eq!(
                candidate.estimate(text, 0),
                Some(oracle.count(text.as_str()) as u64),
                "{} case {index}",
                candidate.name()
            );
        }
    }
}
