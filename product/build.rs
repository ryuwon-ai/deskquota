fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    #[cfg(feature = "bpe")]
    dictionaries::generate();
}

#[cfg(feature = "bpe")]
mod dictionaries {
    use serde::{Deserialize, de::IgnoredAny};
    use std::{collections::BTreeMap, fs, io::Cursor, path::Path};

    // Mirror of bpe 0.2.2's serialized fields, only at build time. Pin both bpe
    // and its DAAC dependency in Cargo.toml; format changes must fail this build.
    #[derive(Deserialize)]
    struct Dictionary {
        all_tokens: Vec<u8>,
        token_starts: Vec<u32>,
        _bytes_hash_to_token: IgnoredAny,
        split_table: Vec<(u32, u32)>,
        pair_lookup: BTreeMap<(u32, u32), u32>,
        #[serde(deserialize_with = "bytes")]
        longest_searcher: Vec<u8>,
        _overlapping_searcher: IgnoredAny,
        _overlapping_searcher_rev: IgnoredAny,
        next_prefix_match: Vec<u32>,
        _hash_factor: IgnoredAny,
    }

    fn bytes<'de, D: serde::Deserializer<'de>>(de: D) -> Result<Vec<u8>, D::Error> {
        struct Bytes;
        impl serde::de::Visitor<'_> for Bytes {
            type Value = Vec<u8>;
            fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                f.write_str("serialized DAAC bytes")
            }
            fn visit_bytes<E: serde::de::Error>(self, value: &[u8]) -> Result<Vec<u8>, E> {
                Ok(value.to_vec())
            }
        }
        de.deserialize_bytes(Bytes)
    }

    fn word(source: &mut &[u8]) -> u32 {
        let (value, rest) = source.split_at(4);
        *source = rest;
        u32::from_le_bytes(value.try_into().expect("four-byte word"))
    }

    fn triples(source: &mut &[u8]) -> Vec<[u32; 3]> {
        let count = word(source) as usize;
        assert!(count <= source.len() / 12, "truncated DAAC table");
        (0..count)
            .map(|_| [word(source), word(source), word(source)])
            .collect()
    }

    pub fn generate() {
        let out_dir = std::env::var_os("OUT_DIR").expect("Cargo OUT_DIR");
        let out = Path::new(&out_dir);
        let mut generated = String::new();
        for (name, tokenizer, expected_tokens) in [
            ("CL100K", bpe_openai::cl100k_base(), 100_256),
            ("O200K", bpe_openai::o200k_base(), 199_998),
        ] {
            let serialized = rmp_serde::to_vec(&tokenizer.bpe).expect("serialize pinned BPE");
            let mut de = rmp_serde::Deserializer::new(Cursor::new(&serialized));
            let dict = Dictionary::deserialize(&mut de).expect("pinned BPE layout");
            assert_eq!(de.get_ref().position() as usize, serialized.len());
            let count = dict.split_table.len();
            assert_eq!(count, expected_tokens);
            assert_eq!(dict.token_starts.len(), count + 1);
            assert_eq!(dict.next_prefix_match.len(), count);
            assert_eq!(dict.token_starts[0], 0);
            assert_eq!(dict.token_starts[count] as usize, dict.all_tokens.len());
            let lengths: Vec<_> = dict
                .token_starts
                .windows(2)
                .map(|w| {
                    assert!(w[0] < w[1]);
                    w[1] - w[0]
                })
                .collect();
            let token = |id: usize| {
                &dict.all_tokens[dict.token_starts[id] as usize..dict.token_starts[id + 1] as usize]
            };
            for (id, &(left, right)) in dict.split_table.iter().enumerate() {
                if left == id as u32 && right == id as u32 {
                    assert_eq!(lengths[id], 1);
                } else {
                    assert!(left < id as u32 && right < id as u32);
                    assert_eq!(
                        token(id),
                        [token(left as usize), token(right as usize)].concat()
                    );
                }
                let prefix = dict.next_prefix_match[id];
                if prefix != u32::MAX {
                    assert!((prefix as usize) < count);
                    assert!(lengths[prefix as usize] < lengths[id]);
                    assert!(token(id).starts_with(token(prefix as usize)));
                }
            }

            // DAAC 1.1.1 serialization: vector<State>, vector<Output>, kind, count.
            let mut source = dict.longest_searcher.as_slice();
            let states = triples(&mut source);
            let outputs = triples(&mut source);
            assert_eq!(source[0], 1, "must be LeftmostLongest");
            source = &source[1..];
            let num_states = word(&mut source);
            assert!(source.is_empty(), "unknown DAAC suffix");
            assert!((num_states as usize) <= states.len());
            assert_eq!(states.len() % 256, 0);
            assert!(states.len() >= 256);
            for &[base, fail, packed] in &states {
                assert!((base as usize) < states.len());
                assert!((fail as usize) < states.len());
                assert!((packed >> 8) as usize <= outputs.len());
            }
            for &[value, length, parent] in &outputs {
                assert!((value as usize) < count);
                assert_eq!(length, lengths[value as usize]);
                assert!(parent as usize <= outputs.len());
            }

            // ponytail: sorted ranges save RAM but cost CPU on dense input;
            // consider a static hash only if profiling makes lookup the bottleneck.
            let mut pair_starts = Vec::with_capacity(count + 1);
            let mut pairs = Vec::with_capacity(dict.pair_lookup.len());
            let mut entries = dict.pair_lookup.iter().peekable();
            for left in 0..count as u32 {
                pair_starts.push(pairs.len() as u32);
                while let Some(&(&(a, b), &value)) = entries.peek() {
                    if a != left {
                        break;
                    }
                    assert!((b as usize) < count && (value as usize) < count);
                    assert_eq!(
                        token(value as usize),
                        [token(a as usize), token(b as usize)].concat()
                    );
                    pairs.push([b, value]);
                    entries.next();
                }
            }
            assert!(entries.next().is_none(), "invalid left token");
            pair_starts.push(pairs.len() as u32);

            generated.push_str(&format!("static {name}: Dictionary = Dictionary {{\n"));
            for (field, words) in [
                ("lengths", lengths),
                (
                    "splits",
                    dict.split_table.iter().flat_map(|&(a, b)| [a, b]).collect(),
                ),
                ("prefixes", dict.next_prefix_match),
                ("pair_starts", pair_starts),
                ("pairs", pairs.into_iter().flatten().collect()),
                ("states", states.into_iter().flatten().collect()),
                ("outputs", outputs.into_iter().map(|o| o[0]).collect()),
            ] {
                let file = format!("{name}_{field}.bin");
                let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
                fs::write(out.join(&file), bytes).expect("write static BPE table");
                generated.push_str(&format!(
                    "{field}: include_bytes!(concat!(env!(\"OUT_DIR\"), \"/{file}\")),\n"
                ));
            }
            generated.push_str("};\n");
        }
        fs::write(out.join("count_dictionaries.rs"), generated)
            .expect("write static BPE references");
    }
}
