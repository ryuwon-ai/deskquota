//! Count-only storage for the pinned rust-gems BPE algorithm. Dictionary tables
//! stay in read-only executable pages; only the selected regex is initialized.
//! Adapted from bpe 0.2.2, bpe-openai 0.3.1 and aneubeck-daachorse 1.1.1.
//! See the repository LICENSE-MIT. Keep differential tests when updating these pins.
use regex_automata::{Anchored, Input, meta::Regex};
use std::sync::LazyLock;

struct Dictionary {
    lengths: &'static [u8],
    splits: &'static [u8],
    prefixes: &'static [u8],
    pair_starts: &'static [u8],
    pairs: &'static [u8],
    states: &'static [u8],
    outputs: &'static [u8],
}

include!(concat!(env!("OUT_DIR"), "/count_dictionaries.rs"));

// Fixed-width LE reads are alignment-independent, bounds-checked and allocate
// nothing. Build-time validation checks every index in these embedded tables.
#[inline]
fn word(table: &[u8], index: usize) -> u32 {
    let start = index * 4;
    u32::from_le_bytes(table[start..start + 4].try_into().expect("static u32"))
}

impl Dictionary {
    fn token_len(&self, token: u32) -> usize {
        word(self.lengths, token as usize) as usize
    }

    fn pair(&self, left: u32, right: u32) -> Option<u32> {
        let mut low = word(self.pair_starts, left as usize) as usize;
        let mut high = word(self.pair_starts, left as usize + 1) as usize;
        while low < high {
            let mid = low + (high - low) / 2;
            match word(self.pairs, mid * 2).cmp(&right) {
                std::cmp::Ordering::Less => low = mid + 1,
                std::cmp::Ordering::Greater => high = mid,
                std::cmp::Ordering::Equal => return Some(word(self.pairs, mid * 2 + 1)),
            }
        }
        None
    }

    fn valid_pair(&self, mut left: u32, mut right: u32) -> bool {
        let mut limit = u32::MAX;
        loop {
            if self
                .pair(left, right)
                .is_some_and(|combined| combined < limit)
            {
                return false;
            }
            if left > right {
                limit = left;
                left = word(self.splits, left as usize * 2 + 1);
                if left == limit {
                    limit = right + 1;
                    right = word(self.splits, right as usize * 2);
                    if right + 1 == limit {
                        return true;
                    }
                }
            } else {
                limit = right + 1;
                right = word(self.splits, right as usize * 2);
                if right + 1 == limit {
                    limit = left;
                    left = word(self.splits, left as usize * 2 + 1);
                    if left == limit {
                        return true;
                    }
                }
            }
        }
    }

    // DAAC's leftmost-longest transition, including failure links. In this
    // format ROOT=0, DEAD=1, check=low byte, output=upper 24 bits (1-based).
    fn next_state(&self, mut state: u32, byte: u8) -> u32 {
        loop {
            let base = word(self.states, state as usize * 3);
            if base != 0 {
                let child = base ^ u32::from(byte);
                if word(self.states, child as usize * 3 + 2) as u8 == byte {
                    return child;
                }
            }
            if state == 0 {
                return 0;
            }
            let fail = word(self.states, state as usize * 3 + 1);
            if fail == 1 {
                return 0;
            }
            state = fail;
        }
    }

    fn next_match(&self, text: &[u8]) -> Option<u32> {
        let mut state = 0;
        let mut last_output = 0;
        for &byte in text {
            state = self.next_state(state, byte);
            if state == 0 {
                if last_output != 0 {
                    break;
                }
            } else {
                let output = word(self.states, state as usize * 3 + 2) >> 8;
                if output != 0 {
                    last_output = output;
                }
            }
        }
        (last_output != 0).then(|| word(self.outputs, last_output as usize - 1))
    }

    fn count(&self, text: &[u8]) -> usize {
        // Preserve upstream's temporary buffers and backtracking rules. This
        // change removes dictionary ownership, not request-local allocations.
        let mut tokens = Vec::with_capacity(text.len() / 3);
        let mut bits = vec![u64::MAX; (text.len() + 1).div_ceil(64)];
        let mut next = self.next_match(text);
        let mut pos = 0;
        while let Some(mut token) = next {
            let last = tokens.last().copied();
            loop {
                let end = pos + self.token_len(token);
                if bits[end / 64] & (1 << (end % 64)) != 0
                    && last.is_none_or(|left| self.valid_pair(left, token))
                {
                    tokens.push(token);
                    pos = end;
                    next = self.next_match(&text[end..]);
                    break;
                }
                let prefix = word(self.prefixes, token as usize);
                if prefix != u32::MAX {
                    token = prefix;
                } else {
                    bits[pos / 64] &= !(1 << (pos % 64));
                    tokens.pop();
                    pos -= last.map(|t| self.token_len(t)).unwrap_or(0);
                    next = last;
                    break;
                }
            }
        }
        tokens.len()
    }
}

pub(super) struct Tokenizer {
    dictionary: &'static Dictionary,
    regex: Regex,
}

impl Tokenizer {
    fn new(dictionary: &'static Dictionary, first_pattern: &str) -> Self {
        Self {
            dictionary,
            regex: Regex::new_many(&[first_pattern, r"\s+\s", r"\s+"])
                .expect("pinned pretokenization regex"),
        }
    }

    pub(super) fn count(&self, text: &str) -> usize {
        let mut last = 0;
        let mut count = 0;
        while let Some(found) = self
            .regex
            .find(Input::new(&text[last..]).anchored(Anchored::Yes))
        {
            let mut end = last + found.end();
            if found.pattern().as_usize() == 1 {
                end -= text[last..end]
                    .chars()
                    .next_back()
                    .expect("lookahead character")
                    .len_utf8();
                assert_ne!(end, last, "lookahead pattern must consume input");
            }
            count += self.dictionary.count(&text.as_bytes()[last..end]);
            last = end;
        }
        count
    }
}

static CL100K_TOKENIZER: LazyLock<Tokenizer> = LazyLock::new(|| {
    Tokenizer::new(
        &CL100K,
        r"(?i:'s|'t|'re|'ve|'m|'ll|'d)|[^\r\n\p{L}\p{N}]?\p{L}+|\p{N}{1,3}| ?[^\s\p{L}\p{N}]+[\r\n]*|\s*[\r\n]+|\s+$",
    )
});
static O200K_TOKENIZER: LazyLock<Tokenizer> = LazyLock::new(|| {
    let pattern = [
        r"[^\r\n\p{L}\p{N}]?[\p{Lu}\p{Lt}\p{Lm}\p{Lo}\p{M}]*[\p{Ll}\p{Lm}\p{Lo}\p{M}]+(?i:'s|'t|'re|'ve|'m|'ll|'d)?",
        r"[^\r\n\p{L}\p{N}]?[\p{Lu}\p{Lt}\p{Lm}\p{Lo}\p{M}]+[\p{Ll}\p{Lm}\p{Lo}\p{M}]*(?i:'s|'t|'re|'ve|'m|'ll|'d)?",
        r"\p{N}{1,3}", r" ?[^\s\p{L}\p{N}]+[\r\n/]*", r"\s*[\r\n]+", r"\s+$",
    ].join("|");
    Tokenizer::new(&O200K, &pattern)
});

pub(super) fn cl100k_base() -> &'static Tokenizer {
    &CL100K_TOKENIZER
}
pub(super) fn o200k_base() -> &'static Tokenizer {
    &O200K_TOKENIZER
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_vocabulary_and_adjacent_tokens_match_upstream() {
        for (candidate, oracle, size) in [
            (&CL100K, bpe_openai::cl100k_base(), 100_256u32),
            (&O200K, bpe_openai::o200k_base(), 199_998u32),
        ] {
            for id in 0..size {
                let bytes = oracle.bpe.token_bytes(id);
                assert_eq!(
                    candidate.count(bytes),
                    oracle.bpe.count(bytes),
                    "vocabulary {size} token {id}"
                );
                // Exercise merge ranking/backtracking on deterministic neighbors,
                // including arbitrary bytes that are not valid Unicode alone.
                let other = (u64::from(id) * 7919 + 17) as u32 % size;
                let pair = [bytes, oracle.bpe.token_bytes(other)].concat();
                assert_eq!(
                    candidate.count(&pair),
                    oracle.bpe.count(&pair),
                    "vocabulary {size} pair {id},{other}"
                );
            }
        }
    }
}
