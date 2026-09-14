use llmgw::config_patch::{
    Edit, EditValue, Format, JournalStatus, KeyPath, Patch, Scope, Transaction, apply,
    cooperating_lock_path, inspect_journal, preview, restore, snapshot_hash,
};
use serde_json::json;
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

const UPSTREAM_SENTINEL: &str = concat!("upstream-", "synthetic-", "8f2f5a47");
const DATA_SENTINEL: &str = concat!("local-data-", "synthetic-", "3dcb99a1");
const CONTROL_SENTINEL: &str = concat!("control-", "synthetic-", "6a01c442");

struct Fixture {
    root: PathBuf,
}

struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn child_mut(&mut self) -> &mut Child {
        self.0.as_mut().expect("child guard is active")
    }

    fn interrupt_and_wait(mut self) -> std::process::ExitStatus {
        let mut child = self.0.take().expect("child guard is active");
        child.kill().unwrap();
        child.wait().unwrap()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(mut child) = self.0.take() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

impl Fixture {
    fn new(name: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "llmgw-patch-{name}-{}-{:016x}",
            std::process::id(),
            rand::random::<u64>()
        ));
        private_dir(&root);
        Self { root }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    fn journal(&self) -> PathBuf {
        self.path("state/patch-journal.json")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            for entry in walk_dirs(&self.root) {
                let _ = fs::set_permissions(entry, fs::Permissions::from_mode(0o700));
            }
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn walk_dirs(root: &Path) -> Vec<PathBuf> {
    let mut found = vec![root.to_path_buf()];
    let mut index = 0;
    while index < found.len() {
        if let Ok(entries) = fs::read_dir(&found[index]) {
            for entry in entries.flatten() {
                if entry.file_type().is_ok_and(|kind| kind.is_dir()) {
                    found.push(entry.path());
                }
            }
        }
        index += 1;
    }
    found
}

fn private_dir(path: &Path) {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(path)
            .unwrap();
    }
    #[cfg(not(unix))]
    fs::create_dir_all(path).unwrap();
}

fn write_private(path: &Path, bytes: &[u8]) {
    private_dir(path.parent().unwrap());
    fs::write(path, bytes).unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
}

fn key(parts: &[&str]) -> KeyPath {
    KeyPath::new(parts.iter().copied()).unwrap()
}

fn set(key: KeyPath, value: EditValue) -> Edit {
    Edit::Set { key, value }
}

fn patch(
    path: &Path,
    format: Format,
    scope: Scope,
    before: Option<&[u8]>,
    edits: Vec<Edit>,
) -> Patch {
    let owned_keys = edits.iter().map(Edit::key).cloned().collect();
    Patch {
        path: path.to_path_buf(),
        expected_hash: snapshot_hash(before),
        format,
        scope,
        edits,
        owned_keys,
        warnings: vec!["gateway must be running for this client route".into()],
    }
}

fn transaction(journal_path: PathBuf, patches: Vec<Patch>) -> Transaction {
    Transaction {
        journal_path,
        patches,
    }
}

fn apply_reviewed(transaction: &Transaction) -> llmgw::config_patch::ApplyReport {
    let reviewed = preview(transaction).unwrap();
    apply(transaction, &reviewed.hash).unwrap()
}

#[test]
fn jsonc_apply_and_restore_preserve_comments_unrelated_keys_and_hide_tokens() {
    let fixture = Fixture::new("jsonc-roundtrip");
    let path = fixture.path("client/models.json");
    let before = format!(
        concat!(
            "{{\n",
            "  // keep this comment\n",
            "  \"unrelated\": {{ \"credential\": \"{}\", \"command\": \"!command\", \"env\": \"${{ENV}}\", \"dollar\": \"$ENV\" }},\n",
            "  \"provider\": {{ \"baseUrl\": \"https://before.invalid\" }}\n",
            "}}\n"
        ),
        UPSTREAM_SENTINEL
    );
    write_private(&path, before.as_bytes());
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::JsonWithComments,
            Scope::UserPrivate,
            Some(before.as_bytes()),
            vec![
                set(
                    key(&["provider", "baseUrl"]),
                    EditValue::Public(json!("http://127.0.0.1:4141/r/pi/v1")),
                ),
                set(
                    key(&["provider", "headers", "X-LLMGW-Token"]),
                    EditValue::LocalDataToken(DATA_SENTINEL.into()),
                ),
            ],
        )],
    );

    let shown = preview(&tx).unwrap();
    assert!(!shown.summary.contains(DATA_SENTINEL));
    assert!(!shown.summary.contains(UPSTREAM_SENTINEL));
    assert!(!shown.summary.contains(CONTROL_SENTINEL));
    let report = apply(&tx, &shown.hash).unwrap();
    assert_eq!(report.applied_resources, 1);
    assert!(!report.summary.contains(DATA_SENTINEL));
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("// keep this comment"));
    assert!(after.contains(UPSTREAM_SENTINEL));
    assert!(after.contains(DATA_SENTINEL));
    assert!(after.contains("!command"));
    assert!(after.contains("${ENV}"));
    assert!(after.contains("$ENV"));

    let journal = fs::read_to_string(fixture.journal()).unwrap();
    for sentinel in [UPSTREAM_SENTINEL, DATA_SENTINEL, CONTROL_SENTINEL] {
        assert!(!journal.contains(sentinel));
        assert!(!report.summary.contains(sentinel));
    }
    let backup = inspect_journal(fixture.journal()).unwrap().resources[0].backup_refs[0].clone();
    assert!(
        fs::read_to_string(backup)
            .unwrap()
            .contains(UPSTREAM_SENTINEL)
    );

    let restored = restore(fixture.journal()).unwrap();
    assert!(restored.conflicts.is_empty());
    let final_text = fs::read_to_string(path).unwrap();
    assert!(final_text.contains("// keep this comment"));
    assert!(final_text.contains("https://before.invalid"));
    assert!(!final_text.contains(DATA_SENTINEL));
    assert!(final_text.contains(UPSTREAM_SENTINEL));
}

#[test]
fn toml_apply_and_restore_preserve_comments_and_unrelated_user_edits() {
    let fixture = Fixture::new("toml-roundtrip");
    let path = fixture.path("client/profile.toml");
    let before = format!(
        "# profile comment\nunrelated = \"keep\"\nexisting_credential = \"{UPSTREAM_SENTINEL}\"\n[provider]\nbase_url = \"https://before.invalid\" # endpoint comment\n"
    );
    write_private(&path, before.as_bytes());
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::Toml,
            Scope::UserPrivate,
            Some(before.as_bytes()),
            vec![set(
                key(&["provider", "base_url"]),
                EditValue::Public(json!("http://127.0.0.1:4141/r/codex")),
            )],
        )],
    );
    apply_reviewed(&tx);
    let after = fs::read_to_string(&path).unwrap();
    assert!(after.contains("# profile comment"));
    assert!(after.contains("# endpoint comment"));

    let user_edited = after.replace("unrelated = \"keep\"", "unrelated = \"user-edited\"");
    fs::write(&path, user_edited).unwrap();
    let restored = restore(fixture.journal()).unwrap();
    assert!(restored.conflicts.is_empty());
    let final_text = fs::read_to_string(path).unwrap();
    assert!(final_text.contains("unrelated = \"user-edited\""));
    assert!(final_text.contains("base_url = \"https://before.invalid\" # endpoint comment"));
}

#[test]
fn toml_owned_datetime_is_refused_before_client_write() {
    let fixture = Fixture::new("toml-owned-datetime");
    let path = fixture.path("client/profile.toml");
    let before = b"last_used = 2026-09-13T07:00:00Z\nkeep = 42\n";
    write_private(&path, before);
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::Toml,
            Scope::UserPrivate,
            Some(before),
            vec![set(
                key(&["last_used"]),
                EditValue::Public(json!("gateway")),
            )],
        )],
    );

    let error = preview(&tx).unwrap_err().to_string();
    assert!(error.contains("TOML owned value"));
    assert!(!error.contains("2026-09-13"));
    assert_eq!(fs::read(&path).unwrap(), before);
    assert!(!fixture.journal().exists());
    let parsed = fs::read_to_string(path)
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    assert!(parsed["last_used"].as_datetime().is_some());
}

#[test]
fn toml_owned_nested_datetime_subtrees_are_refused() {
    for (index, temporal) in [
        "1979-05-27",
        "07:32:00",
        "1979-05-27T07:32:00",
        "1979-05-27T07:32:00Z",
    ]
    .into_iter()
    .enumerate()
    {
        let fixture = Fixture::new(&format!("toml-owned-nested-datetime-{index}"));
        let path = fixture.path("client/profile.toml");
        let before =
            format!("profile = {{ model = \"original\", schedule = [{temporal}] }}\nkeep = true\n");
        write_private(&path, before.as_bytes());
        let tx = transaction(
            fixture.journal(),
            vec![patch(
                &path,
                Format::Toml,
                Scope::UserPrivate,
                Some(before.as_bytes()),
                vec![set(
                    key(&["profile"]),
                    EditValue::Public(json!({"model":"gateway"})),
                )],
            )],
        );

        let error = preview(&tx).unwrap_err().to_string();
        assert!(error.contains("TOML owned value"));
        assert!(!error.contains(temporal));
        assert_eq!(fs::read(&path).unwrap(), before.as_bytes());
        assert!(!fixture.journal().exists());
    }
}

#[test]
fn toml_owned_nonfinite_scalars_are_refused_before_state_or_client_write() {
    for (index, literal) in ["nan", "+inf", "-inf"].into_iter().enumerate() {
        let fixture = Fixture::new(&format!("toml-owned-nonfinite-{index}"));
        let path = fixture.path("client/profile.toml");
        let before = format!("value = {literal}\nkeep = true\n");
        write_private(&path, before.as_bytes());
        let tx = transaction(
            fixture.journal(),
            vec![patch(
                &path,
                Format::Toml,
                Scope::UserPrivate,
                Some(before.as_bytes()),
                vec![set(key(&["value"]), EditValue::Public(json!(1.0)))],
            )],
        );

        let preview_error = preview(&tx).unwrap_err().to_string();
        let apply_error = apply(&tx, "invalid-preview-hash").unwrap_err().to_string();
        for error in [preview_error, apply_error] {
            assert!(error.contains("TOML owned value"));
            assert!(!error.contains(literal));
        }
        assert_eq!(fs::read(&path).unwrap(), before.as_bytes());
        assert!(!fixture.path("state").exists());
    }
}

#[test]
fn toml_owned_nested_nonfinite_replacement_and_removal_are_refused() {
    let cases = [
        (
            "profile = { model = \"original\", limits = [1.0, nan] }\nkeep = true\n",
            set(
                key(&["profile"]),
                EditValue::Public(json!({"model":"gateway"})),
            ),
        ),
        (
            "[[profiles]]\nname = \"first\"\nlimit = +inf\n\n[[profiles]]\nname = \"second\"\nlimit = 2.0\n",
            Edit::Remove {
                key: key(&["profiles"]),
            },
        ),
    ];
    for (index, (before, edit)) in cases.into_iter().enumerate() {
        let fixture = Fixture::new(&format!("toml-owned-nested-nonfinite-{index}"));
        let path = fixture.path("client/profile.toml");
        write_private(&path, before.as_bytes());
        let tx = transaction(
            fixture.journal(),
            vec![patch(
                &path,
                Format::Toml,
                Scope::UserPrivate,
                Some(before.as_bytes()),
                vec![edit],
            )],
        );

        let error = preview(&tx).unwrap_err().to_string();
        assert!(error.contains("TOML owned value"));
        assert!(!error.contains("nan"));
        assert!(!error.contains("inf"));
        assert_eq!(fs::read(&path).unwrap(), before.as_bytes());
        assert!(!fixture.path("state").exists());
    }
}

#[test]
fn unrelated_nonfinite_and_owned_finite_toml_floats_roundtrip() {
    let fixture = Fixture::new("toml-supported-floats");
    let path = fixture.path("client/profile.toml");
    let before = b"external = -inf\nvalue = 1.25\nmodel = \"original\"\n";
    write_private(&path, before);
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::Toml,
            Scope::UserPrivate,
            Some(before),
            vec![
                set(key(&["value"]), EditValue::Public(json!(2.5))),
                set(key(&["model"]), EditValue::Public(json!("gateway"))),
            ],
        )],
    );
    apply_reviewed(&tx);
    assert!(restore(fixture.journal()).unwrap().conflicts.is_empty());
    assert_eq!(fs::read(&path).unwrap(), before);
    let parsed = fs::read_to_string(path)
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    assert!(parsed["external"].as_float().unwrap().is_infinite());
    assert_eq!(parsed["value"].as_float(), Some(1.25));
    assert_eq!(parsed["model"].as_str(), Some("original"));
}

#[test]
fn unrelated_toml_datetime_survives_normal_string_edit_and_restore() {
    let fixture = Fixture::new("toml-unrelated-datetime");
    let path = fixture.path("client/profile.toml");
    let before = b"last_used = 2026-09-13T07:00:00Z\nmodel = \"original\"\n";
    write_private(&path, before);
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::Toml,
            Scope::UserPrivate,
            Some(before),
            vec![set(key(&["model"]), EditValue::Public(json!("gateway")))],
        )],
    );
    apply_reviewed(&tx);
    assert!(restore(fixture.journal()).unwrap().conflicts.is_empty());
    assert_eq!(fs::read(&path).unwrap(), before);
    let parsed = fs::read_to_string(path)
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    assert!(parsed["last_used"].as_datetime().is_some());
    assert_eq!(parsed["model"].as_str(), Some("original"));
}

#[test]
fn ordinary_toml_marker_shaped_table_is_not_misclassified_as_datetime() {
    let fixture = Fixture::new("toml-marker-table");
    let path = fixture.path("client/profile.toml");
    let before = b"value = { \"$__toml_private_datetime\" = \"ordinary-data\" }\n";
    write_private(&path, before);
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::Toml,
            Scope::UserPrivate,
            Some(before),
            vec![set(key(&["value"]), EditValue::Public(json!("gateway")))],
        )],
    );
    apply_reviewed(&tx);
    assert!(restore(fixture.journal()).unwrap().conflicts.is_empty());
    let parsed = fs::read_to_string(path)
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    let value = parsed["value"].as_inline_table().unwrap();
    assert_eq!(
        value
            .get("$__toml_private_datetime")
            .and_then(toml_edit::Value::as_str),
        Some("ordinary-data")
    );
}

#[test]
fn strict_json_and_comments_only_json_refuse_ambiguous_or_unsupported_grammar() {
    let fixture = Fixture::new("json-grammar");
    let cases = [
        (Format::StrictJson, "{ // no\n \"a\": 1 }"),
        (Format::JsonWithComments, "{ \"a\": 1, }"),
        (Format::JsonWithComments, "{ 'a': 1 }"),
        (Format::JsonWithComments, "{ \"a\": 1 \"b\": 2 }"),
        (Format::JsonWithComments, "{ \"a\": 1, \"a\": 2 }"),
        (
            Format::JsonWithComments,
            "{ \"a\": [[{ \"nested\": 1, \"nested\": 2 }]] }",
        ),
    ];
    for (index, (format, source)) in cases.into_iter().enumerate() {
        let path = fixture.path(&format!("client/{index}.json"));
        write_private(&path, source.as_bytes());
        let tx = transaction(
            fixture.path(&format!("state/{index}.json")),
            vec![patch(
                &path,
                format,
                Scope::UserPrivate,
                Some(source.as_bytes()),
                vec![set(key(&["a"]), EditValue::Public(json!(3)))],
            )],
        );
        let debug = format!("{tx:?}");
        assert!(!debug.contains(DATA_SENTINEL));
        assert!(!debug.contains(UPSTREAM_SENTINEL));
        assert!(!debug.contains(CONTROL_SENTINEL));
        let error = preview(&tx).unwrap_err().to_string();
        assert!(
            error.contains("parse") || error.contains("duplicate"),
            "{error}"
        );
        assert_eq!(fs::read_to_string(path).unwrap(), source);
    }
}

#[test]
fn json_validation_rejects_non_json_whitespace_and_raw_string_controls() {
    let fixture = Fixture::new("json-scanner-boundaries");
    let cases: [&[u8]; 4] = [
        b"{\x0b\"a\": 1}",
        b"{\x0c\"a\": 1}",
        b"{\"a\": \"line\nbreak\"}",
        b"{\"a\": \"raw\x01control\"}",
    ];
    for (index, source) in cases.into_iter().enumerate() {
        let path = fixture.path(&format!("client/{index}.json"));
        write_private(&path, source);
        let tx = transaction(
            fixture.path(&format!("state/{index}.json")),
            vec![patch(
                &path,
                Format::JsonWithComments,
                Scope::UserPrivate,
                Some(source),
                vec![set(key(&["a"]), EditValue::Public(json!(2)))],
            )],
        );
        assert!(
            preview(&tx).is_err(),
            "invalid JSON boundary {index} passed"
        );
        assert_eq!(fs::read(path).unwrap(), source);
    }
}

#[test]
fn comments_only_json_keeps_comment_markers_inside_strings_as_data() {
    let fixture = Fixture::new("json-comment-ranges");
    let path = fixture.path("client/models.json");
    let before = br#"{
  // a real comment
  "url": "https://example.invalid/a//b",
  "block": "/* data */",
  "model": "before"
}"#;
    write_private(&path, before);
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::JsonWithComments,
            Scope::UserPrivate,
            Some(before),
            vec![set(key(&["model"]), EditValue::Public(json!("after")))],
        )],
    );
    apply_reviewed(&tx);
    let after = fs::read_to_string(path).unwrap();
    assert!(after.contains("// a real comment"));
    assert!(after.contains("https://example.invalid/a//b"));
    assert!(after.contains("/* data */"));
}

#[test]
fn apply_requires_exact_preview_hash_and_rechecks_concurrent_edits() {
    let fixture = Fixture::new("hash-contract");
    let path = fixture.path("client/settings.json");
    let before = br#"{"model":"before","unrelated":1}"#;
    write_private(&path, before);
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![set(key(&["model"]), EditValue::Public(json!("gateway")))],
        )],
    );
    let shown = preview(&tx).unwrap();
    let error = apply(&tx, "reviewed-a-different-preview").unwrap_err();
    assert!(error.to_string().contains("preview hash"));
    assert_eq!(fs::read(&path).unwrap(), before);

    fs::write(&path, br#"{"model":"user","unrelated":2}"#).unwrap();
    let error = apply(&tx, &shown.hash).unwrap_err();
    assert!(error.to_string().contains("changed"));
    assert!(error.to_string().contains("new preview"));
    assert_eq!(
        fs::read(&path).unwrap(),
        br#"{"model":"user","unrelated":2}"#
    );
}

#[test]
fn cooperating_editors_share_one_lock_namespace_across_different_journals() {
    use fs4::FileExt;
    use std::fs::OpenOptions;

    let fixture = Fixture::new("canonical-lock");
    let path = fixture.path("client/settings.json");
    let alias = fixture.path("client/../client/settings.json");
    let before = br#"{"model":"before"}"#;
    write_private(&path, before);
    let tx = transaction(
        fixture.path("a-state/journal.json"),
        vec![patch(
            &alias,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![set(key(&["model"]), EditValue::Public(json!("after")))],
        )],
    );
    let shown = preview(&tx).unwrap();
    let lock_path = cooperating_lock_path(&path).unwrap();
    private_dir(lock_path.parent().unwrap());
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&lock_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    FileExt::lock(&lock).unwrap();
    let error = apply(&tx, &shown.hash).unwrap_err().to_string();
    assert!(error.contains("another llmgw client resource edit"));
    assert_eq!(fs::read(&path).unwrap(), before);
    drop(lock);
    fs::remove_file(&lock_path).unwrap();
    let _ = fs::remove_dir(lock_path.parent().unwrap());
}

#[test]
fn resource_lock_identity_does_not_depend_on_process_temp_directory() {
    let fixture = Fixture::new("temp-independent-lock");
    let path = fixture.path("client/settings.json");
    write_private(&path, br#"{"model":"before"}"#);
    let first_temp = fixture.path("process-temp-a");
    let second_temp = fixture.path("process-temp-b");
    private_dir(&first_temp);
    private_dir(&second_temp);
    let first_output = fixture.path("lock-a.txt");
    let second_output = fixture.path("lock-b.txt");

    for (temp, output) in [(&first_temp, &first_output), (&second_temp, &second_output)] {
        let status = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg("config_patch_lock_path_child")
            .arg("--nocapture")
            .env("LLMGW_PATCH_LOCK_RESOURCE", &path)
            .env("LLMGW_PATCH_LOCK_OUTPUT", output)
            .env("TMPDIR", temp)
            .env("TMP", temp)
            .env("TEMP", temp)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert!(status.success());
    }

    assert_eq!(
        fs::read(&first_output).unwrap(),
        fs::read(&second_output).unwrap()
    );
}

#[test]
fn config_patch_lock_path_child() {
    let Some(resource) = std::env::var_os("LLMGW_PATCH_LOCK_RESOURCE") else {
        return;
    };
    let output = PathBuf::from(std::env::var_os("LLMGW_PATCH_LOCK_OUTPUT").unwrap());
    fs::write(
        output,
        cooperating_lock_path(Path::new(&resource))
            .unwrap()
            .to_string_lossy()
            .as_bytes(),
    )
    .unwrap();
}

#[test]
fn restore_uses_the_same_resource_identity_as_an_alias_apply() {
    use fs4::FileExt;
    use std::fs::OpenOptions;

    let fixture = Fixture::new("alias-restore-lock");
    let path = fixture.path("client/settings.json");
    let alias = fixture.path("client/../client/settings.json");
    let before = br#"{"model":"before"}"#;
    write_private(&path, before);
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &alias,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![set(key(&["model"]), EditValue::Public(json!("after")))],
        )],
    );
    apply_reviewed(&tx);

    let lock_path = cooperating_lock_path(&path).unwrap();
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&lock_path)
        .unwrap();
    FileExt::lock(&lock).unwrap();
    let error = restore(fixture.journal()).unwrap_err().to_string();
    assert!(error.contains("another llmgw client resource edit"));
    assert!(fs::read_to_string(&path).unwrap().contains("after"));
    drop(lock);
    assert!(restore(fixture.journal()).unwrap().conflicts.is_empty());
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn case_aliases_share_a_lock_when_the_filesystem_equates_them() {
    use fs4::FileExt;
    use std::fs::OpenOptions;

    let fixture = Fixture::new("case-alias-lock");
    let path = fixture.path("client/settings.json");
    let alias = fixture.path("client/SETTINGS.JSON");
    let before = br#"{"model":"before"}"#;
    write_private(&path, before);
    if fs::canonicalize(&alias).ok() != fs::canonicalize(&path).ok() {
        return;
    }

    let held_path = cooperating_lock_path(&path).unwrap();
    let alias_path = cooperating_lock_path(&alias).unwrap();
    assert_eq!(held_path, alias_path);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&held_path)
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&held_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    FileExt::lock(&lock).unwrap();
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &alias,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![set(key(&["model"]), EditValue::Public(json!("after")))],
        )],
    );
    let shown = preview(&tx).unwrap();
    let error = apply(&tx, &shown.hash).unwrap_err().to_string();
    assert!(error.contains("another llmgw client resource edit"));
    assert_eq!(fs::read(path).unwrap(), before);
}

#[test]
fn absent_case_aliases_contend_on_the_actual_filesystem_lock() {
    assert_absent_aliases_contend("SETTINGS.JSON", "settings.json", "case");
}

#[test]
fn absent_unicode_aliases_contend_when_the_filesystem_equates_them() {
    assert_absent_aliases_contend("caf\u{e9}.json", "cafe\u{301}.json", "unicode");
}

#[test]
fn missing_resource_lock_uses_the_adjacent_filename_and_fixed_suffix() {
    let fixture = Fixture::new("missing-resource-lock-name");
    let parent = fixture.path("client");
    private_dir(&parent);
    let path = parent.join("settings.json");
    let lock_path = cooperating_lock_path(&path).unwrap();
    assert_eq!(
        lock_path.parent().unwrap(),
        fs::canonicalize(parent).unwrap()
    );
    assert_eq!(
        lock_path.file_name().unwrap(),
        "settings.json.llmgw-config-patch.lock"
    );
}

fn assert_absent_aliases_contend(first_name: &str, second_name: &str, label: &str) {
    use fs4::FileExt;
    use std::fs::OpenOptions;

    let fixture = Fixture::new(&format!("absent-{label}-alias-lock"));
    let parent = fixture.path("client");
    private_dir(&parent);
    let marker = parent.join(first_name);
    write_private(&marker, b"marker");
    let aliases_match = parent.join(second_name).exists();
    fs::remove_file(&marker).unwrap();
    if !aliases_match {
        return;
    }

    let first = parent.join(first_name);
    let second = parent.join(second_name);
    let held_path = cooperating_lock_path(&first).unwrap();
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&held_path)
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&held_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    FileExt::lock(&lock).unwrap();
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &second,
            Format::StrictJson,
            Scope::UserPrivate,
            None,
            vec![set(key(&["model"]), EditValue::Public(json!("gateway")))],
        )],
    );
    let shown = preview(&tx).unwrap();
    let error = apply(&tx, &shown.hash).unwrap_err().to_string();
    assert!(error.contains("another llmgw client resource edit"));
    assert!(!first.exists());
    assert!(!second.exists());
}

#[test]
fn absent_case_distinct_files_keep_independent_locks_on_case_sensitive_filesystems() {
    use fs4::FileExt;
    use std::fs::OpenOptions;

    let fixture = Fixture::new("absent-case-distinct-lock");
    let parent = fixture.path("client");
    private_dir(&parent);
    let upper = parent.join("SETTINGS.JSON");
    let lower = parent.join("settings.json");
    write_private(&upper, b"marker");
    let aliases_match = lower.exists();
    fs::remove_file(&upper).unwrap();
    if aliases_match {
        return;
    }

    let held_path = cooperating_lock_path(&upper).unwrap();
    let lower_path = cooperating_lock_path(&lower).unwrap();
    assert_ne!(held_path, lower_path);
    let lock = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&held_path)
        .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&held_path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    FileExt::lock(&lock).unwrap();
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &lower,
            Format::StrictJson,
            Scope::UserPrivate,
            None,
            vec![set(key(&["model"]), EditValue::Public(json!("gateway")))],
        )],
    );
    apply_reviewed(&tx);
    assert!(lower.exists());
}

#[cfg(unix)]
#[test]
fn adjacent_resource_lock_preserves_an_existing_ordinary_parent_mode() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("ordinary-lock-parent");
    let parent = fixture.path("client");
    let path = parent.join("settings.json");
    let before = br#"{"model":"before"}"#;
    write_private(&path, before);
    fs::set_permissions(&parent, fs::Permissions::from_mode(0o755)).unwrap();
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::ProjectShared,
            Some(before),
            vec![set(key(&["model"]), EditValue::Public(json!("after")))],
        )],
    );

    apply_reviewed(&tx);
    assert_eq!(
        fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
        0o755
    );
    restore(fixture.journal()).unwrap();
    assert_eq!(
        fs::metadata(&parent).unwrap().permissions().mode() & 0o777,
        0o755
    );
}

#[test]
fn unresolved_alias_through_a_missing_parent_is_refused_before_writing() {
    let fixture = Fixture::new("unsupported-missing-alias");
    let path = fixture.path("missing/../client/new.json");
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            None,
            vec![set(key(&["model"]), EditValue::Public(json!("after")))],
        )],
    );

    let error = preview(&tx).unwrap_err().to_string();
    assert!(error.contains("unresolved parent aliases"));
    assert!(!fixture.path("client/new.json").exists());
}

#[test]
fn apply_and_restore_use_the_same_transaction_lock() {
    use fs4::FileExt;
    use std::fs::OpenOptions;

    let fixture = Fixture::new("transaction-lock");
    let path = fixture.path("client/settings.json");
    let before = br#"{"model":"before"}"#;
    write_private(&path, before);
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![set(key(&["model"]), EditValue::Public(json!("after")))],
        )],
    );
    apply_reviewed(&tx);
    let journal_name = fixture.journal().file_name().unwrap().to_os_string();
    let mut lock_name = journal_name;
    lock_name.push(".lock");
    let lock_path = fixture.journal().with_file_name(lock_name);
    let lock = OpenOptions::new()
        .read(true)
        .write(true)
        .open(lock_path)
        .unwrap();
    FileExt::lock(&lock).unwrap();
    let error = restore(fixture.journal()).unwrap_err().to_string();
    assert!(error.contains("another llmgw transaction edit"));
    assert!(fs::read_to_string(&path).unwrap().contains("after"));
    drop(lock);
    assert!(restore(fixture.journal()).unwrap().conflicts.is_empty());
    assert_eq!(fs::read(path).unwrap(), before);
}

#[cfg(unix)]
#[test]
fn local_token_requires_existing_user_private_file_without_changing_mode() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("private-token");
    let path = fixture.path("client/settings.json");
    let before = br#"{"env":{}}"#;
    write_private(&path, before);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![set(
                key(&["env", "X_LLMGW_TOKEN"]),
                EditValue::LocalDataToken(DATA_SENTINEL.into()),
            )],
        )],
    );
    let shown = preview(&tx).unwrap();
    let error = apply(&tx, &shown.hash).unwrap_err().to_string();
    assert!(error.contains("readable by other users"));
    assert!(error.contains("correct the permission"));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o644
    );
    assert_eq!(fs::read(&path).unwrap(), before);

    #[cfg(target_os = "macos")]
    {
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        assert!(
            Command::new("/bin/chmod")
                .args(["+a", "everyone allow read"])
                .arg(&path)
                .status()
                .unwrap()
                .success()
        );
        let acl = exacl::getfacl(&path, exacl::AclOption::SYMLINK_ACL).unwrap();
        let error = apply(&tx, &shown.hash).unwrap_err().to_string();
        assert!(error.contains("extended ACL"));
        assert_eq!(
            exacl::getfacl(&path, exacl::AclOption::SYMLINK_ACL).unwrap(),
            acl
        );
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn project_shared_token_and_non_data_credentials_are_refused_before_writing() {
    let fixture = Fixture::new("credential-boundary");
    let cases = [
        (
            Scope::ProjectShared,
            EditValue::LocalDataToken(DATA_SENTINEL.into()),
        ),
        (
            Scope::UserPrivate,
            EditValue::UpstreamCredential(UPSTREAM_SENTINEL.into()),
        ),
        (
            Scope::UserPrivate,
            EditValue::ControlToken(CONTROL_SENTINEL.into()),
        ),
    ];
    for (index, (scope, value)) in cases.into_iter().enumerate() {
        let path = fixture.path(&format!("client/{index}.json"));
        let before = br#"{"env":{}}"#;
        write_private(&path, before);
        let tx = transaction(
            fixture.path(&format!("state/{index}.json")),
            vec![patch(
                &path,
                Format::StrictJson,
                scope,
                Some(before),
                vec![set(key(&["env", "TOKEN"]), value)],
            )],
        );
        let debug = format!("{tx:?}");
        assert!(!debug.contains(DATA_SENTINEL));
        assert!(!debug.contains(UPSTREAM_SENTINEL));
        assert!(!debug.contains(CONTROL_SENTINEL));
        let error = preview(&tx).unwrap_err().to_string();
        assert!(!error.contains(DATA_SENTINEL));
        assert!(!error.contains(UPSTREAM_SENTINEL));
        assert!(!error.contains(CONTROL_SENTINEL));
        assert_eq!(fs::read(&path).unwrap(), before);
    }
}

#[test]
fn typed_secret_debug_diff_output_journal_and_summaries_are_redacted() {
    let fixture = Fixture::new("typed-secret-surfaces");
    let output = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("config_patch_redaction_child")
        .arg("--nocapture")
        .env("LLMGW_PATCH_REDACTION_FIXTURE", &fixture.root)
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    let journal = fs::read_to_string(fixture.journal()).unwrap();
    assert!(stdout.contains("[REDACTED]"));
    for sentinel in [UPSTREAM_SENTINEL, DATA_SENTINEL, CONTROL_SENTINEL] {
        assert!(!stdout.contains(sentinel));
        assert!(!stderr.contains(sentinel));
        assert!(!journal.contains(sentinel));
    }
}

#[test]
fn config_patch_redaction_child() {
    let Some(root) = std::env::var_os("LLMGW_PATCH_REDACTION_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let path = root.join("client/settings.json");
    let journal_path = root.join("state/patch-journal.json");
    let before = br#"{"env":{}}"#;
    write_private(&path, before);
    let local = transaction(
        journal_path.clone(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![set(
                key(&["env", "TOKEN"]),
                EditValue::LocalDataToken(DATA_SENTINEL.into()),
            )],
        )],
    );
    println!("transaction={local:?}");
    let shown = preview(&local).unwrap();
    println!("preview={shown:?}");
    println!("apply={:?}", apply(&local, &shown.hash).unwrap());
    println!("doctor={:?}", inspect_journal(&journal_path).unwrap());
    println!("restore={:?}", restore(&journal_path).unwrap());

    for (index, value) in [
        EditValue::UpstreamCredential(UPSTREAM_SENTINEL.into()),
        EditValue::ControlToken(CONTROL_SENTINEL.into()),
    ]
    .into_iter()
    .enumerate()
    {
        let rejected = transaction(
            root.join(format!("state/rejected-{index}.json")),
            vec![patch(
                &path,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(before),
                vec![set(key(&["env", "TOKEN"]), value)],
            )],
        );
        println!("rejected={rejected:?}");
        let error = preview(&rejected).unwrap_err();
        println!("error-debug={error:?}");
        eprintln!("error={error}");
    }
}

#[cfg(unix)]
#[test]
fn partial_two_file_apply_is_explicit_and_recoverable() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("partial");
    let first = fixture.path("client-a/settings.json");
    let second = fixture.path("client-b/settings.json");
    let before_a = br#"{"model":"a"}"#;
    let before_b = br#"{"model":"b"}"#;
    write_private(&first, before_a);
    write_private(&second, before_b);
    fs::set_permissions(second.parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
    let tx = transaction(
        fixture.journal(),
        vec![
            patch(
                &first,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(before_a),
                vec![set(key(&["model"]), EditValue::Public(json!("gateway-a")))],
            ),
            patch(
                &second,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(before_b),
                vec![set(key(&["model"]), EditValue::Public(json!("gateway-b")))],
            ),
        ],
    );
    let shown = preview(&tx).unwrap();
    let error = apply(&tx, &shown.hash).unwrap_err().to_string();
    assert!(error.contains("partial"));
    assert!(error.contains(&fixture.journal().display().to_string()));
    assert!(fs::read_to_string(&first).unwrap().contains("gateway-a"));
    assert_eq!(fs::read(&second).unwrap(), before_b);
    let journal = inspect_journal(fixture.journal()).unwrap();
    assert_eq!(journal.status, JournalStatus::Partial);
    assert!(journal.recovery.contains("restore"));
    assert_eq!(journal.resources.len(), 2);
    assert_eq!(journal.resources[0].stage.as_str(), "verified");
    assert_ne!(journal.resources[1].stage.as_str(), "verified");
    fs::set_permissions(second.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    let restored = restore(fixture.journal()).unwrap();
    assert!(restored.conflicts.is_empty());
    assert_eq!(fs::read(first).unwrap(), before_a);
    assert_eq!(fs::read(second).unwrap(), before_b);
}

#[cfg(unix)]
#[test]
fn partial_three_file_apply_records_and_preserves_every_reviewed_target() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("partial-three");
    let paths = (0..3)
        .map(|index| fixture.path(&format!("client-{index}/settings.json")))
        .collect::<Vec<_>>();
    let before = br#"{"model":"original"}"#;
    for path in &paths {
        write_private(path, before);
    }
    fs::set_permissions(
        paths[1].parent().unwrap(),
        fs::Permissions::from_mode(0o500),
    )
    .unwrap();
    let tx = transaction(
        fixture.journal(),
        paths
            .iter()
            .map(|path| {
                patch(
                    path,
                    Format::StrictJson,
                    Scope::UserPrivate,
                    Some(before),
                    vec![set(key(&["model"]), EditValue::Public(json!("gateway")))],
                )
            })
            .collect(),
    );
    let shown = preview(&tx).unwrap();
    let error = apply(&tx, &shown.hash).unwrap_err().to_string();
    assert!(error.contains("partial"));

    let journal = inspect_journal(fixture.journal()).unwrap();
    assert_eq!(journal.resources.len(), 3);
    assert_eq!(
        journal
            .resources
            .iter()
            .map(|resource| resource.stage.as_str())
            .collect::<Vec<_>>(),
        ["verified", "failed", "unstarted"]
    );
    assert_eq!(
        journal
            .resources
            .iter()
            .map(|resource| resource.owned_keys.len())
            .collect::<Vec<_>>(),
        [1, 0, 0]
    );
    assert!(journal.resources[2].backup_refs.is_empty());
    assert_eq!(fs::read(&paths[2]).unwrap(), before);

    fs::set_permissions(
        paths[1].parent().unwrap(),
        fs::Permissions::from_mode(0o700),
    )
    .unwrap();
    let restored = restore(fixture.journal()).unwrap();
    assert_eq!(restored.restored_resources, 1);
    assert!(restored.conflicts.is_empty());
    for path in paths {
        assert_eq!(fs::read(path).unwrap(), before);
    }
    let journal = inspect_journal(fixture.journal()).unwrap();
    assert!(journal.recovery.contains("never owned"));
}

#[cfg(unix)]
#[test]
fn failed_unowned_absent_target_uses_user_file_as_first_preownership_snapshot() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("failed-unowned-absent-retry");
    let path = fixture.path("client/settings.json");
    private_dir(path.parent().unwrap());
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
    let first = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            None,
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v1")))],
        )],
    );
    let shown = preview(&first).unwrap();
    assert!(apply(&first, &shown.hash).is_err());
    assert!(!path.exists());
    let failed = inspect_journal(fixture.journal()).unwrap();
    assert_eq!(failed.resources[0].stage.as_str(), "failed");
    assert!(failed.resources[0].owned_keys.is_empty());

    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    let user_created = br#"{"model":"user-original","user_added":"preserve-me"}"#;
    write_private(&path, user_created);
    let retry = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(user_created),
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v2")))],
        )],
    );
    apply_reviewed(&retry);
    let restored = restore(fixture.journal()).unwrap();
    assert!(restored.conflicts.is_empty());
    assert!(path.exists());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(value["model"], "user-original");
    assert_eq!(value["user_added"], "preserve-me");
}

#[cfg(unix)]
#[test]
fn later_unstarted_absent_target_uses_user_file_as_first_preownership_snapshot() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("later-unstarted-absent-retry");
    let first = fixture.path("client-a/settings.json");
    let blocked = fixture.path("client-b/settings.json");
    let later = fixture.path("client-c/settings.json");
    let original = br#"{"model":"original"}"#;
    write_private(&first, original);
    write_private(&blocked, original);
    private_dir(later.parent().unwrap());
    fs::set_permissions(blocked.parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
    let initial = transaction(
        fixture.journal(),
        vec![
            patch(
                &first,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(original),
                vec![set(key(&["model"]), EditValue::Public(json!("gateway-a")))],
            ),
            patch(
                &blocked,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(original),
                vec![set(key(&["model"]), EditValue::Public(json!("gateway-b")))],
            ),
            patch(
                &later,
                Format::StrictJson,
                Scope::UserPrivate,
                None,
                vec![set(key(&["model"]), EditValue::Public(json!("gateway-c")))],
            ),
        ],
    );
    let shown = preview(&initial).unwrap();
    assert!(apply(&initial, &shown.hash).is_err());
    let partial = inspect_journal(fixture.journal()).unwrap();
    assert_eq!(partial.resources[2].stage.as_str(), "unstarted");
    assert!(partial.resources[2].owned_keys.is_empty());

    fs::set_permissions(blocked.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    let first_written = fs::read(&first).unwrap();
    let user_created = br#"{"model":"user-original","user_added":"preserve-me"}"#;
    write_private(&later, user_created);
    let retry = transaction(
        fixture.journal(),
        vec![
            patch(
                &first,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(&first_written),
                vec![set(
                    key(&["model"]),
                    EditValue::Public(json!("gateway-a-v2")),
                )],
            ),
            patch(
                &blocked,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(original),
                vec![set(key(&["model"]), EditValue::Public(json!("gateway-b")))],
            ),
            patch(
                &later,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(user_created),
                vec![set(key(&["model"]), EditValue::Public(json!("gateway-c")))],
            ),
        ],
    );
    apply_reviewed(&retry);
    let restored = restore(fixture.journal()).unwrap();
    assert!(restored.conflicts.is_empty());
    assert_eq!(fs::read(first).unwrap(), original);
    assert_eq!(fs::read(blocked).unwrap(), original);
    let value: serde_json::Value = serde_json::from_slice(&fs::read(later).unwrap()).unwrap();
    assert_eq!(value["model"], "user-original");
    assert_eq!(value["user_added"], "preserve-me");
}

#[cfg(unix)]
#[test]
fn failed_unowned_existing_target_removed_before_retry_is_owned_as_created() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("failed-unowned-existing-retry");
    let path = fixture.path("client/settings.json");
    let original = br#"{"model":"user-original","unrelated":true}"#;
    write_private(&path, original);
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
    let first = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(original),
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v1")))],
        )],
    );
    let shown = preview(&first).unwrap();
    assert!(apply(&first, &shown.hash).is_err());
    let failed = inspect_journal(fixture.journal()).unwrap();
    assert!(failed.resources[0].owned_keys.is_empty());

    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    fs::remove_file(&path).unwrap();
    let retry = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            None,
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v2")))],
        )],
    );
    apply_reviewed(&retry);
    restore(fixture.journal()).unwrap();
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn partial_two_file_restore_is_explicit_and_recoverable() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("partial-restore");
    let first = fixture.path("client-a/settings.json");
    let second = fixture.path("client-b/settings.json");
    let before_a = br#"{"model":"a"}"#;
    let before_b = br#"{"model":"b"}"#;
    write_private(&first, before_a);
    write_private(&second, before_b);
    let tx = transaction(
        fixture.journal(),
        vec![
            patch(
                &first,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(before_a),
                vec![set(key(&["model"]), EditValue::Public(json!("gateway-a")))],
            ),
            patch(
                &second,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(before_b),
                vec![set(key(&["model"]), EditValue::Public(json!("gateway-b")))],
            ),
        ],
    );
    apply_reviewed(&tx);
    fs::set_permissions(second.parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
    let error = restore(fixture.journal()).unwrap_err().to_string();
    assert!(error.contains("restore is partial"));
    assert_eq!(fs::read(&first).unwrap(), before_a);
    assert!(fs::read_to_string(&second).unwrap().contains("gateway-b"));
    let journal = inspect_journal(fixture.journal()).unwrap();
    assert_eq!(journal.status, JournalStatus::RestorePartial);
    assert!(journal.recovery.contains("restore did not finish"));

    fs::set_permissions(second.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    assert!(restore(fixture.journal()).unwrap().conflicts.is_empty());
    assert_eq!(fs::read(second).unwrap(), before_b);
}

#[test]
fn interrupted_created_file_reconnect_keeps_verified_removal_boundary() {
    let fixture = Fixture::new("created-reconnect-crash");
    let created = fixture.path("client-created/settings.json");
    let first = transaction(
        fixture.journal(),
        vec![patch(
            &created,
            Format::StrictJson,
            Scope::UserPrivate,
            None,
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v1")))],
        )],
    );
    apply_reviewed(&first);
    for index in 0..64 {
        write_private(
            &fixture.path(&format!("clients/{index:02}.json")),
            br#"{"model":"original"}"#,
        );
    }

    let child = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("config_patch_created_reconnect_crash_child")
        .arg("--nocapture")
        .env("LLMGW_PATCH_CREATED_RECONNECT_CRASH_FIXTURE", &fixture.root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard::new(child);
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if fs::read_to_string(&created).is_ok_and(|text| text.contains("gateway-v2")) {
            break;
        }
        if let Some(status) = child.child_mut().try_wait().unwrap() {
            panic!("created reconnect crash fixture exited before interruption: {status}");
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(fs::read_to_string(&created).unwrap().contains("gateway-v2"));
    assert!(!child.interrupt_and_wait().success());

    let restored = restore(fixture.journal()).unwrap();
    assert!(restored.conflicts.is_empty());
    assert!(!created.exists());
}

#[test]
fn config_patch_created_reconnect_crash_child() {
    let Some(root) = std::env::var_os("LLMGW_PATCH_CREATED_RECONNECT_CRASH_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let created = root.join("client-created/settings.json");
    let mut patches = vec![patch(
        &created,
        Format::StrictJson,
        Scope::UserPrivate,
        fs::read(&created).ok().as_deref(),
        vec![set(key(&["model"]), EditValue::Public(json!("gateway-v2")))],
    )];
    patches.extend((0..64).map(|index| {
        let path = root.join(format!("clients/{index:02}.json"));
        patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            fs::read(&path).ok().as_deref(),
            vec![set(
                key(&["model"]),
                EditValue::Public(json!(format!("gateway-{index:02}"))),
            )],
        )
    }));
    let tx = transaction(root.join("state/patch-journal.json"), patches);
    apply_reviewed(&tx);
}

#[test]
fn interrupted_process_leaves_stageful_journal_and_restores_written_resources() {
    let fixture = Fixture::new("crash-parent");
    let before = br#"{"model":"original"}"#;
    for index in 0..64 {
        write_private(&fixture.path(&format!("clients/{index:02}.json")), before);
    }
    let child = Command::new(std::env::current_exe().unwrap())
        .arg("--exact")
        .arg("config_patch_crash_child")
        .arg("--nocapture")
        .env("LLMGW_PATCH_CRASH_FIXTURE", &fixture.root)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut child = ChildGuard::new(child);
    let first = fixture.path("clients/00.json");
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if fs::read_to_string(&first).is_ok_and(|text| text.contains("gateway-00")) {
            break;
        }
        if let Some(status) = child.child_mut().try_wait().unwrap() {
            panic!("crash fixture exited before interruption: {status}");
        }
        thread::sleep(Duration::from_millis(1));
    }
    assert!(fs::read_to_string(&first).unwrap().contains("gateway-00"));
    let status = child.interrupt_and_wait();
    assert!(!status.success());

    let inspected = inspect_journal(fixture.journal()).unwrap();
    assert_eq!(inspected.status, JournalStatus::Applying);
    assert!(inspected.recovery.contains("interrupted"));
    assert!(inspected.resources.iter().any(|resource| matches!(
        resource.stage.as_str(),
        "backed_up" | "written" | "verified"
    )));
    let restored = restore(fixture.journal()).unwrap();
    assert!(restored.conflicts.is_empty());
    for index in 0..64 {
        assert_eq!(
            fs::read(fixture.path(&format!("clients/{index:02}.json"))).unwrap(),
            before
        );
    }
}

#[test]
fn config_patch_crash_child() {
    let Some(root) = std::env::var_os("LLMGW_PATCH_CRASH_FIXTURE") else {
        return;
    };
    let root = PathBuf::from(root);
    let before = br#"{"model":"original"}"#;
    let patches = (0..64)
        .map(|index| {
            let path = root.join(format!("clients/{index:02}.json"));
            patch(
                &path,
                Format::StrictJson,
                Scope::UserPrivate,
                Some(before),
                vec![set(
                    key(&["model"]),
                    EditValue::Public(json!(format!("gateway-{index:02}"))),
                )],
            )
        })
        .collect();
    let tx = transaction(root.join("state/patch-journal.json"), patches);
    apply_reviewed(&tx);
}

#[cfg(unix)]
#[test]
fn ordinary_non_token_patch_preserves_existing_mode_and_macos_acl() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("mode-preservation");
    let path = fixture.path("client/settings.json");
    let before = br#"{"model":"before"}"#;
    write_private(&path, before);
    fs::set_permissions(&path, fs::Permissions::from_mode(0o640)).unwrap();
    #[cfg(target_os = "macos")]
    assert!(
        Command::new("/bin/chmod")
            .args(["+a", "everyone allow read"])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    #[cfg(target_os = "macos")]
    let acl_before = {
        let acl = exacl::getfacl(&path, exacl::AclOption::SYMLINK_ACL).unwrap();
        assert!(!acl.is_empty());
        acl
    };
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![set(key(&["model"]), EditValue::Public(json!("after")))],
        )],
    );
    apply_reviewed(&tx);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o640
    );
    #[cfg(target_os = "macos")]
    assert_eq!(
        exacl::getfacl(&path, exacl::AclOption::SYMLINK_ACL).unwrap(),
        acl_before
    );
}

#[test]
fn repeated_connect_keeps_first_preownership_value_and_one_resource_record() {
    let fixture = Fixture::new("repeat-connect");
    let path = fixture.path("client/settings.json");
    let first_before = br#"{"model":"original","unrelated":true}"#;
    write_private(&path, first_before);
    let first = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(first_before),
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v1")))],
        )],
    );
    apply_reviewed(&first);

    let second_before = fs::read(&path).unwrap();
    let alias = fixture.path("client/../client/settings.json");
    let second = transaction(
        fixture.journal(),
        vec![patch(
            &alias,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(&second_before),
            vec![
                set(key(&["model"]), EditValue::Public(json!("gateway-v2"))),
                set(key(&["new_owned"]), EditValue::Public(json!(true))),
            ],
        )],
    );
    apply_reviewed(&second);
    let journal = inspect_journal(fixture.journal()).unwrap();
    assert_eq!(journal.resources.len(), 1);
    assert_eq!(journal.resources[0].owned_keys.len(), 2);

    let restored = restore(fixture.journal()).unwrap();
    assert!(restored.conflicts.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(value["model"], "original");
    assert!(value.get("new_owned").is_none());
    assert_eq!(value["unrelated"], true);
}

#[test]
fn created_file_user_field_and_comment_survive_reconnect_and_restore() {
    let fixture = Fixture::new("created-reconnect-user-edit");
    let path = fixture.path("client/settings.json");
    let first = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::JsonWithComments,
            Scope::UserPrivate,
            None,
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v1")))],
        )],
    );
    apply_reviewed(&first);

    let user_edited = br#"{
  // user comment must survive
  "model": "gateway-v1",
  "user_added": "preserve-me"
}
"#;
    fs::write(&path, user_edited).unwrap();
    let second = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::JsonWithComments,
            Scope::UserPrivate,
            Some(user_edited),
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v2")))],
        )],
    );
    apply_reviewed(&second);

    let report = restore(fixture.journal()).unwrap();
    assert!(report.conflicts.is_empty());
    assert!(path.exists());
    let restored = fs::read_to_string(path).unwrap();
    assert!(restored.contains("// user comment must survive"));
    assert!(restored.contains("preserve-me"));
    assert!(!restored.contains("gateway-v2"));
}

#[test]
fn unchanged_created_file_remains_removable_after_reconnect() {
    let fixture = Fixture::new("created-reconnect-unchanged");
    let path = fixture.path("client/settings.json");
    let first = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            None,
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v1")))],
        )],
    );
    apply_reviewed(&first);
    let first_written = fs::read(&path).unwrap();
    let second = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(&first_written),
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v2")))],
        )],
    );
    apply_reviewed(&second);
    restore(fixture.journal()).unwrap();
    assert!(!path.exists());
}

#[cfg(unix)]
#[test]
fn failed_created_file_reconnect_keeps_last_verified_removal_boundary() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("created-reconnect-failed");
    let path = fixture.path("client/settings.json");
    let first = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            None,
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v1")))],
        )],
    );
    apply_reviewed(&first);
    let first_written = fs::read(&path).unwrap();
    let second = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(&first_written),
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v2")))],
        )],
    );
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
    let shown = preview(&second).unwrap();
    assert!(apply(&second, &shown.hash).is_err());
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(fs::read(&path).unwrap(), first_written);
    restore(fixture.journal()).unwrap();
    assert!(!path.exists());
}

#[test]
fn reconnect_rejects_descendant_then_ancestor_ownership_overlap() {
    assert_reconnect_overlap_is_rejected(
        &["provider", "model"],
        json!("gateway"),
        &["provider"],
        json!({"model":"gateway","other":"changed"}),
    );
}

#[test]
fn reconnect_rejects_ancestor_then_descendant_ownership_overlap() {
    assert_reconnect_overlap_is_rejected(
        &["provider"],
        json!({"model":"gateway","other":"keep"}),
        &["provider", "model"],
        json!("gateway-v2"),
    );
}

fn assert_reconnect_overlap_is_rejected(
    first_key: &[&str],
    first_value: serde_json::Value,
    second_key: &[&str],
    second_value: serde_json::Value,
) {
    let fixture = Fixture::new("reconnect-overlap");
    let path = fixture.path("client/settings.json");
    let original = br#"{"provider":{"model":"original","other":"keep"}}"#;
    write_private(&path, original);
    let first = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(original),
            vec![set(key(first_key), EditValue::Public(first_value))],
        )],
    );
    apply_reviewed(&first);
    let first_written = fs::read(&path).unwrap();
    let second = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(&first_written),
            vec![set(key(second_key), EditValue::Public(second_value))],
        )],
    );
    let shown = preview(&second).unwrap();
    let error = apply(&second, &shown.hash).unwrap_err().to_string();
    assert!(error.contains("overlaps an existing owned key"));
    assert!(!error.contains("gateway"));
    assert_eq!(fs::read(&path).unwrap(), first_written);

    let restored = restore(fixture.journal()).unwrap();
    assert!(restored.conflicts.is_empty());
    let final_value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let original_value: serde_json::Value = serde_json::from_slice(original).unwrap();
    assert_eq!(final_value, original_value);
}

#[cfg(unix)]
#[test]
fn failed_repeated_connect_keeps_the_last_verified_owned_value_recoverable() {
    use std::os::unix::fs::PermissionsExt;

    let fixture = Fixture::new("repeat-failure");
    let path = fixture.path("client/settings.json");
    let original = br#"{"model":"original"}"#;
    write_private(&path, original);
    let first = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(original),
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v1")))],
        )],
    );
    apply_reviewed(&first);
    let verified_v1 = fs::read(&path).unwrap();

    let second = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(&verified_v1),
            vec![set(key(&["model"]), EditValue::Public(json!("gateway-v2")))],
        )],
    );
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
    let shown = preview(&second).unwrap();
    assert!(apply(&second, &shown.hash).is_err());
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    assert_eq!(fs::read(&path).unwrap(), verified_v1);

    let restored = restore(fixture.journal()).unwrap();
    assert!(restored.conflicts.is_empty());
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(value["model"], "original");
}

#[test]
fn disconnect_preserves_user_owned_key_change_but_restores_other_owned_keys() {
    let fixture = Fixture::new("restore-conflict");
    let path = fixture.path("client/settings.json");
    let before = br#"{"model":"original","env":{"TOKEN":"old"},"unrelated":1}"#;
    write_private(&path, before);
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![
                set(key(&["model"]), EditValue::Public(json!("gateway"))),
                set(
                    key(&["env", "TOKEN"]),
                    EditValue::LocalDataToken(DATA_SENTINEL.into()),
                ),
            ],
        )],
    );
    apply_reviewed(&tx);
    let user_changed =
        format!(r#"{{"model":"user-choice","env":{{"TOKEN":"{DATA_SENTINEL}"}},"unrelated":2}}"#);
    fs::write(&path, user_changed).unwrap();

    let report = restore(fixture.journal()).unwrap();
    assert_eq!(report.conflicts, vec![key(&["model"])]);
    let value: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    assert_eq!(value["model"], "user-choice");
    assert_eq!(value["env"]["TOKEN"], "old");
    assert_eq!(value["unrelated"], 2);
}

#[test]
fn newly_created_file_is_removed_only_while_its_exact_bytes_are_owned() {
    let exact = Fixture::new("new-exact");
    let exact_path = exact.path("client/new.json");
    let exact_tx = transaction(
        exact.journal(),
        vec![patch(
            &exact_path,
            Format::StrictJson,
            Scope::UserPrivate,
            None,
            vec![set(key(&["provider"]), EditValue::Public(json!("gateway")))],
        )],
    );
    apply_reviewed(&exact_tx);
    assert!(exact_path.exists());
    restore(exact.journal()).unwrap();
    assert!(!exact_path.exists());

    let changed = Fixture::new("new-changed");
    let changed_path = changed.path("client/new.json");
    let changed_tx = transaction(
        changed.journal(),
        vec![patch(
            &changed_path,
            Format::StrictJson,
            Scope::UserPrivate,
            None,
            vec![set(key(&["provider"]), EditValue::Public(json!("gateway")))],
        )],
    );
    apply_reviewed(&changed_tx);
    let mut user_changed = fs::read_to_string(&changed_path).unwrap();
    user_changed.push('\n');
    fs::write(&changed_path, user_changed.as_bytes()).unwrap();
    let normalized_changed_path = fs::canonicalize(&changed_path).unwrap();
    let report = restore(changed.journal()).unwrap();
    assert_eq!(
        report.preserved_created_files,
        vec![normalized_changed_path]
    );
    assert_eq!(fs::read(changed_path).unwrap(), user_changed.as_bytes());
}

#[test]
fn local_data_token_object_is_private_opaque_and_restores_as_one_owned_value() {
    let fixture = Fixture::new("local-token-object");
    let path = fixture.path("client/models.json");
    let before = br#"{"providers":{"other":{"models":[]}}}"#;
    write_private(&path, before);
    let provider = json!({
        "baseUrl": "http://127.0.0.1:4141/r/pi/v1",
        "headers": {"X-LLMGW-Token": DATA_SENTINEL},
        "models": [{"id":"fixture"}]
    });
    let tx = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![set(
                key(&["providers", "llmgw"]),
                EditValue::LocalDataTokenObject(provider),
            )],
        )],
    );
    let shown = preview(&tx).unwrap();
    assert!(!shown.summary.contains(DATA_SENTINEL));
    assert!(!format!("{:?}", tx.patches[0].edits[0]).contains(DATA_SENTINEL));
    let alternate_provider = json!({
        "baseUrl": "http://127.0.0.1:4141/r/pi/v1",
        "headers": {"X-LLMGW-Token": "different-local-token"},
        "models": [{"id":"fixture"}]
    });
    let alternate = transaction(
        fixture.journal(),
        vec![patch(
            &path,
            Format::StrictJson,
            Scope::UserPrivate,
            Some(before),
            vec![set(
                key(&["providers", "llmgw"]),
                EditValue::LocalDataTokenObject(alternate_provider),
            )],
        )],
    );
    let alternate_preview = preview(&alternate).unwrap();
    assert_ne!(shown.hash, alternate_preview.hash);
    assert!(!alternate_preview.summary.contains("different-local-token"));
    apply(&tx, &shown.hash).unwrap();
    assert!(fs::read_to_string(&path).unwrap().contains(DATA_SENTINEL));
    restore(fixture.journal()).unwrap();
    let restored: serde_json::Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
    let original: serde_json::Value = serde_json::from_slice(before).unwrap();
    assert_eq!(restored, original);
}
