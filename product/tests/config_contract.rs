use std::fs;
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use llmgw::config::{Accounting, Auth, CancelPolicy, Endpoint, Limit, LoadedConfig, Method};

const VALID_CONFIG: &str = r#"
listen = "127.0.0.1:4141"
accounting = "reserved"

[upstream]
api_base = "https://api.example.test/team/v1?api-version=2026-09"

[upstream.auth]
mode = "env"
header = "Authorization"
name = "LLMGW_TEST_TOKEN"

[quota.rpm]
kind = "known"
value = 60

[quota.tpm]
kind = "unknown"

[[models]]
id = "bounded-model"
max_output_tokens = 4096

[[models]]
id = "request-bounded-model"

[[roots]]
id = "pi-work"
endpoints = ["chat/completions", "responses", "messages", "messages/count_tokens", "models"]
models = ["bounded-model", "request-bounded-model"]
"#;

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempConfig {
    directory: PathBuf,
    path: PathBuf,
}

impl TempConfig {
    fn new(contents: &str) -> Self {
        let sequence = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "llmgw-config-contract-{}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("create isolated test directory");
        let path = directory.join("gateway.toml");
        fs::write(&path, contents).expect("write synthetic test config");
        Self { directory, path }
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn rewrite(&self, contents: &str) {
        fs::write(&self.path, contents).expect("rewrite synthetic test config");
    }
}

impl Drop for TempConfig {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

fn load(contents: &str) -> LoadedConfig {
    let fixture = TempConfig::new(contents);
    LoadedConfig::load(fixture.path()).expect("configuration should be valid")
}

fn error(contents: &str) -> String {
    let fixture = TempConfig::new(contents);
    LoadedConfig::load(fixture.path())
        .expect_err("configuration should be rejected")
        .to_string()
}

fn rendered_error(contents: &str) -> String {
    let fixture = TempConfig::new(contents);
    let error = LoadedConfig::load(fixture.path()).expect_err("configuration should be rejected");
    format!("{error}\n{error:?}")
}

fn replace_once(source: &str, from: &str, to: &str) -> String {
    assert!(
        source.contains(from),
        "test setup must replace an existing fragment"
    );
    source.replacen(from, to, 1)
}

#[test]
fn valid_config_preserves_limits_auth_accounting_and_url() {
    let loaded = load(VALID_CONFIG);

    assert_eq!(
        loaded.config.quota.rpm,
        Limit::Known(NonZeroU64::new(60).expect("nonzero test value"))
    );
    assert_eq!(loaded.config.quota.tpm, Limit::Unknown);
    assert_eq!(loaded.config.accounting, Accounting::Reserved);
    assert_eq!(
        loaded.config.upstream.auth,
        Auth::Env {
            header: "Authorization".to_owned(),
            name: "LLMGW_TEST_TOKEN".to_owned(),
        }
    );
    assert_eq!(
        loaded.config.upstream.api_base.as_str(),
        "https://api.example.test/team/v1?api-version=2026-09"
    );
}

#[test]
fn concurrency_defaults_to_one_and_is_bounded() {
    assert_eq!(load(VALID_CONFIG).config.concurrency, 1);

    let sixteen = replace_once(
        VALID_CONFIG,
        "accounting =",
        "concurrency = 16\naccounting =",
    );
    assert_eq!(load(&sixteen).config.concurrency, 16);

    for invalid in [0, 17] {
        let config = replace_once(
            VALID_CONFIG,
            "accounting =",
            &format!("concurrency = {invalid}\naccounting ="),
        );
        assert!(error(&config).contains("concurrency"));
    }
}

#[test]
fn cancel_policy_defaults_to_drain_and_accepts_only_close() {
    assert_eq!(load(VALID_CONFIG).config.cancel_policy, CancelPolicy::Drain);

    let close = replace_once(
        VALID_CONFIG,
        "accounting =",
        "cancel_policy = \"close\"\naccounting =",
    );
    assert_eq!(load(&close).config.cancel_policy, CancelPolicy::Close);

    let invalid = replace_once(
        VALID_CONFIG,
        "accounting =",
        "cancel_policy = \"abort\"\naccounting =",
    );
    assert!(error(&invalid).contains("cancel_policy"));
}

#[test]
fn listen_address_must_be_loopback() {
    let config = VALID_CONFIG.replace("127.0.0.1:4141", "0.0.0.0:4141");
    assert!(error(&config).contains("loopback"));
}

#[test]
fn quota_modes_are_distinct_and_known_zero_is_rejected() {
    let unlimited = VALID_CONFIG.replace(
        "[quota.tpm]\nkind = \"unknown\"",
        "[quota.tpm]\nkind = \"unlimited\"",
    );
    assert_eq!(load(&unlimited).config.quota.tpm, Limit::Unlimited);

    let zero = VALID_CONFIG.replace("value = 60", "value = 0");
    let message = error(&zero);
    assert!(message.contains("rpm"));
    assert!(message.contains("nonzero"));
}

#[test]
fn known_tpm_without_a_default_output_bound_is_valid() {
    let config = VALID_CONFIG.replace(
        "[quota.tpm]\nkind = \"unknown\"",
        "[quota.tpm]\nkind = \"known\"\nvalue = 120000",
    );
    let loaded = load(&config);

    assert_eq!(
        loaded.config.quota.tpm,
        Limit::Known(NonZeroU64::new(120_000).expect("nonzero test value"))
    );
    assert_eq!(loaded.config.models[1].max_output_tokens, None);
}

#[test]
fn model_output_bound_is_optional_but_cannot_be_zero() {
    let loaded = load(VALID_CONFIG);
    assert_eq!(
        loaded.config.models[0].max_output_tokens,
        NonZeroU64::new(4096)
    );
    assert_eq!(loaded.config.models[1].max_output_tokens, None);

    let zero = VALID_CONFIG.replace("max_output_tokens = 4096", "max_output_tokens = 0");
    assert!(error(&zero).contains("max_output_tokens"));
}

#[test]
fn root_ids_must_be_unique_url_safe_and_limited_to_sixteen() {
    let duplicate = format!(
        "{VALID_CONFIG}\n[[roots]]\nid = \"pi-work\"\nendpoints = [\"responses\"]\nmodels = [\"bounded-model\"]\n"
    );
    assert!(error(&duplicate).contains("duplicate root id"));

    for unsafe_id in ["", "space name", "slash/name", ".hidden", "한글"] {
        let config = VALID_CONFIG.replace("id = \"pi-work\"", &format!("id = \"{unsafe_id}\""));
        assert!(error(&config).contains("root id"));
    }

    let mut too_many = VALID_CONFIG.to_owned();
    for index in 1..=16 {
        too_many.push_str(&format!(
            "\n[[roots]]\nid = \"root-{index}\"\nendpoints = [\"models\"]\nmodels = [\"bounded-model\"]\n"
        ));
    }
    assert!(error(&too_many).contains("at most 16 roots"));
}

#[test]
fn endpoints_are_an_explicit_method_and_path_allowlist() {
    let endpoints = &load(VALID_CONFIG).config.roots[0].endpoints;
    assert_eq!(endpoints.len(), 5);
    assert!(endpoints.contains(&Endpoint::ChatCompletions));
    assert!(endpoints.contains(&Endpoint::Responses));
    assert!(endpoints.contains(&Endpoint::Messages));
    assert!(endpoints.contains(&Endpoint::CountTokens));
    assert!(endpoints.contains(&Endpoint::Models));
    for endpoint in endpoints {
        let expected = if endpoint.path() == "models" {
            Method::Get
        } else {
            Method::Post
        };
        assert_eq!(endpoint.method(), expected);
    }

    let unsupported = VALID_CONFIG.replace("\"responses\"", "\"completions\"");
    assert!(error(&unsupported).contains("unsupported endpoint"));
}

#[test]
fn api_base_rejects_credentials_non_http_urls_and_fragments() {
    for (url, expected) in [
        ("https://user:password@example.test/v1", "credentials"),
        ("ftp://example.test/v1", "http"),
        ("not a url", "URL"),
        ("https://example.test/v1#fragment", "fragment"),
    ] {
        let config =
            VALID_CONFIG.replace("https://api.example.test/team/v1?api-version=2026-09", url);
        assert!(error(&config).contains(expected), "URL case: {url}");
    }
}

#[test]
fn auth_modes_require_only_their_own_fields() {
    for mode in ["forward", "none"] {
        let config = VALID_CONFIG.replace(
            "mode = \"env\"\nheader = \"Authorization\"\nname = \"LLMGW_TEST_TOKEN\"",
            &format!("mode = \"{mode}\""),
        );
        let expected = if mode == "forward" {
            Auth::Forward
        } else {
            Auth::None
        };
        assert_eq!(load(&config).config.upstream.auth, expected);
    }

    let missing_name = VALID_CONFIG.replace("name = \"LLMGW_TEST_TOKEN\"\n", "");
    assert!(error(&missing_name).contains("name"));
}

#[test]
fn actual_accounting_is_explicitly_supported() {
    let actual = VALID_CONFIG.replace("accounting = \"reserved\"", "accounting = \"actual\"");
    assert_eq!(load(&actual).config.accounting, Accounting::Actual);
}

#[test]
fn unknown_fields_are_rejected_without_echoing_the_key_or_input() {
    let secret = "never-print-this-query-value";
    let unknown_key = "unexpected_secret_setting";
    let config = replace_once(
        &VALID_CONFIG.replace("2026-09", secret),
        "accounting =",
        &format!("{unknown_key} = true\naccounting ="),
    );
    let rendered = rendered_error(&config);
    assert!(rendered.contains("invalid configuration"));
    assert!(!rendered.contains(unknown_key));
    assert!(!rendered.contains(secret));
    assert!(!rendered.contains("api_base"));
}

#[test]
fn invalid_scalar_values_are_redacted_from_display_and_debug() {
    let secret = "https://company.example/private?token=never-print-this";
    let cases = [
        replace_once(
            VALID_CONFIG,
            "accounting =",
            &format!("concurrency = {secret:?}\naccounting ="),
        ),
        VALID_CONFIG.replace("value = 60", &format!("value = {secret:?}")),
        VALID_CONFIG.replace(
            "accounting = \"reserved\"",
            &format!("accounting = {secret:?}"),
        ),
        VALID_CONFIG.replace("mode = \"env\"", &format!("mode = {secret:?}")),
        VALID_CONFIG.replace("\"responses\"", &format!("{secret:?}")),
        VALID_CONFIG.replace("id = \"bounded-model\"", &format!("id = {secret:?}")),
        VALID_CONFIG.replace("id = \"pi-work\"", &format!("id = {secret:?}")),
    ];

    for config in cases {
        let rendered = rendered_error(&config);
        assert!(!rendered.contains(secret));
        assert!(!rendered.contains("company.example"));
        assert!(!rendered.contains("never-print-this"));
    }
}

#[test]
fn syntax_errors_report_a_safe_location_without_source_text() {
    let secret = "https://company.example/private?token=syntax-secret";
    let config = format!("{VALID_CONFIG}\n# {secret}\nbroken = [\n");
    let rendered = rendered_error(&config);

    assert!(rendered.contains("invalid configuration"));
    assert!(rendered.contains("line"));
    assert!(rendered.contains("column"));
    assert!(!rendered.contains(secret));
    assert!(!rendered.contains("company.example"));
    assert!(!rendered.contains("syntax-secret"));
}

#[test]
fn config_debug_redacts_upstream_query_values() {
    let secret = "debug-must-not-show-this";
    let loaded = load(&VALID_CONFIG.replace("2026-09", secret));
    let debug = format!("{:?}", loaded.config);

    assert!(!debug.contains(secret));
    assert!(!debug.contains("api-version"));
    assert!(!debug.contains("api.example.test"));
    assert!(!debug.contains("team/v1"));
    assert!(debug.contains("<redacted>"));
}

#[test]
fn canonical_source_fingerprint_and_state_paths_share_the_explicit_location() {
    let fixture = TempConfig::new(VALID_CONFIG);
    let first = LoadedConfig::load(fixture.path()).expect("first load");
    let expected_path = fs::canonicalize(fixture.path()).expect("canonical test path");
    let expected_state_dir = expected_path
        .parent()
        .expect("test config parent")
        .join(format!(
            ".llmgw-{}",
            llmgw::config::StatePaths::path_hash(&expected_path)
        ));

    assert_eq!(first.source_path, expected_path);
    assert_eq!(first.state_paths.directory, expected_state_dir);
    assert!(!first.state_paths.directory.join("data-token").exists());
    assert!(
        first
            .state_paths
            .control_token
            .starts_with(&first.state_paths.directory)
    );
    assert!(
        first
            .state_paths
            .runtime_state
            .starts_with(&first.state_paths.directory)
    );
    assert_eq!(first.fingerprint.as_str().len(), 64);

    fixture.rewrite(&format!("{VALID_CONFIG}\n# byte-level change\n"));
    let second = LoadedConfig::load(fixture.path()).expect("second load");
    assert_ne!(first.fingerprint, second.fingerprint);
    assert_eq!(first.state_paths, second.state_paths);
    let other = fixture.directory.join("other.toml");
    fs::write(&other, VALID_CONFIG).unwrap();
    assert_ne!(
        first.state_paths,
        LoadedConfig::load(other).unwrap().state_paths
    );
}

#[test]
fn env_auth_rejects_transport_reserved_headers() {
    for header in [
        "Host",
        "Connection",
        "Transfer-Encoding",
        "X-LLMGW-Token",
        "X-LLMGW-Control-Token",
    ] {
        let config = VALID_CONFIG.replace(
            "header = \"Authorization\"",
            &format!("header = {header:?}"),
        );
        assert!(error(&config).contains("reserved"), "header {header}");
    }
}

#[test]
fn lossless_query_rejects_a_raw_config_query_that_url_would_transform() {
    let config = VALID_CONFIG.replace("api-version=2026-09", "trace=a'b");
    let message = error(&config);

    assert!(message.contains("upstream.api_base"));
    assert!(message.contains("query"));
    assert!(!message.contains("a'b"));

    for query in ["trace=a%27b", "trace=a+b", "trace=a%2bb"] {
        let config = VALID_CONFIG.replace("api-version=2026-09", query);
        let loaded = load(&config);
        assert_eq!(loaded.config.upstream.api_base.query(), Some(query));
    }

    let empty = VALID_CONFIG.replace(
        "https://api.example.test/team/v1?api-version=2026-09",
        "https://api.example.test/team/v1?",
    );
    assert!(error(&empty).contains("query"));
}

#[test]
fn defaults_use_actual_and_startup_hold_is_bounded() {
    let source = VALID_CONFIG.replace("accounting = \"reserved\"", "");
    let config = llmgw::config::parse(source.as_bytes()).unwrap();
    assert_eq!(config.accounting, Accounting::Actual);
    assert_eq!(config.startup_hold_secs, 60);
    for seconds in [0, 17, 3600] {
        let parsed =
            llmgw::config::parse(format!("startup_hold_secs = {seconds}\n{source}").as_bytes())
                .unwrap();
        assert_eq!(parsed.startup_hold_secs, seconds);
    }
    for value in ["-1", "3601", "18446744073709551615", "1.5"] {
        assert!(
            llmgw::config::parse(format!("startup_hold_secs = {value}\n{source}").as_bytes())
                .is_err()
        );
    }
}

#[test]
fn exact_cache_table_is_optional_bounded_and_rejects_unknown_fields() {
    assert!(load(VALID_CONFIG).config.cache.is_none());
    let configured = |fields: &str| format!("{VALID_CONFIG}\n[cache]\n{fields}\n");
    assert_eq!(
        load(&configured("")).config.cache,
        Some(llmgw::config::CacheConfig::default())
    );
    for fields in [
        "ttl_secs = 0",
        "ttl_secs = 3601",
        "ttl_secs = -1",
        "max_history = 0",
        "max_history = 65",
        "capacity = 5",
        "ttl_secs = 1.5",
    ] {
        assert!(
            llmgw::config::parse(configured(fields).as_bytes()).is_err(),
            "{fields}"
        );
    }
    assert_eq!(
        load(&configured("ttl_secs = 1\nmax_history = 64"))
            .config
            .cache,
        Some(llmgw::config::CacheConfig {
            ttl_secs: 1,
            max_history: 64
        })
    );
}
