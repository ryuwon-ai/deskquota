//! Multilingual estimates of normalized JSON, not the provider's rendered prompt.
use serde::{Deserialize, Serialize};

#[cfg(feature = "bpe")]
mod compact_bpe;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum InputEstimator {
    #[cfg_attr(not(feature = "bpe"), default)]
    Utf8Bytes,
    #[cfg_attr(feature = "bpe", default)]
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

    pub const fn is_available(self) -> bool {
        matches!(self, Self::Utf8Bytes) || cfg!(feature = "bpe")
    }

    /// Initialize only the selected shared vocabulary before accepting requests.
    pub(crate) fn prepare(self) {
        match self {
            Self::Utf8Bytes => {}
            #[cfg(feature = "bpe")]
            Self::Cl100kBase => {
                compact_bpe::cl100k_base();
            }
            #[cfg(feature = "bpe")]
            Self::O200kBase => {
                compact_bpe::o200k_base();
            }
            #[cfg(not(feature = "bpe"))]
            Self::Cl100kBase | Self::O200kBase => {}
        }
    }

    /// Decode escaped Unicode and remove outside whitespace without converting
    /// opaque numbers or building a recursive JSON tree. Key order and duplicate
    /// opaque keys are preserved. The original request is forwarded unchanged.
    pub fn estimate_json(self, json: &str, overhead: u64) -> Option<u64> {
        if self == Self::Utf8Bytes {
            return self.estimate(json, overhead);
        }
        // IgnoredAny validates strict JSON iteratively, without Value's nesting
        // and floating-point limits. The existing scanner handles string escapes.
        serde_json::from_str::<serde::de::IgnoredAny>(json).ok()?;
        let mut scanner = jsonc_parser::Scanner::new(
            json,
            &jsonc_parser::ScannerOptions {
                allow_single_quoted_strings: false,
                allow_hexadecimal_numbers: false,
                allow_unary_plus_numbers: false,
            },
        );
        let mut normalized = Vec::with_capacity(json.len());
        while let Some(token) = scanner.scan().ok()? {
            match token {
                jsonc_parser::tokens::Token::String(value) => {
                    serde_json::to_writer(&mut normalized, &value).ok()?;
                }
                token => normalized.extend_from_slice(token.as_str().as_bytes()),
            }
        }
        self.estimate(std::str::from_utf8(&normalized).ok()?, overhead)
    }

    pub fn estimate(self, json: &str, overhead: u64) -> Option<u64> {
        let count = match self {
            Self::Utf8Bytes if overhead != 0 => return None,
            Self::Utf8Bytes => json.len(),
            #[cfg(feature = "bpe")]
            Self::Cl100kBase => compact_bpe::cl100k_base().count(json),
            #[cfg(feature = "bpe")]
            Self::O200kBase => compact_bpe::o200k_base().count(json),
            #[cfg(not(feature = "bpe"))]
            Self::Cl100kBase | Self::O200kBase => return None,
        };
        u64::try_from(count).ok()?.checked_add(overhead)
    }
}
