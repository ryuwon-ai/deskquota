//! Counts the original serialized JSON, not the provider's rendered prompt.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InputEstimator {
    #[default]
    Utf8Bytes,
    Cl100kBase,
    O200kBase,
}

impl InputEstimator {
    pub const fn name(self) -> &'static str {
        match self {
            Self::Utf8Bytes => "utf8_bytes",
            Self::Cl100kBase => "cl100k_base",
            Self::O200kBase => "o200k_base",
        }
    }

    pub const fn default_overhead(self) -> u64 {
        match self {
            Self::Utf8Bytes => 0,
            Self::Cl100kBase | Self::O200kBase => 32,
        }
    }

    /// Initialize only the selected shared vocabulary before accepting requests.
    pub(crate) fn prepare(self) {
        match self {
            Self::Utf8Bytes => {}
            Self::Cl100kBase => {
                bpe_openai::cl100k_base();
            }
            Self::O200kBase => {
                bpe_openai::o200k_base();
            }
        }
    }

    pub fn estimate(self, json: &str, overhead: u64) -> Option<u64> {
        let count = match self {
            Self::Utf8Bytes if overhead != 0 => return None,
            Self::Utf8Bytes => json.len(),
            Self::Cl100kBase => bpe_openai::cl100k_base().count(json),
            Self::O200kBase => bpe_openai::o200k_base().count(json),
        };
        u64::try_from(count).ok()?.checked_add(overhead)
    }
}
