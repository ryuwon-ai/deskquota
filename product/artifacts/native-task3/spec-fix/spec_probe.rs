use llmgw::config_patch::{apply, inspect_journal, preview, restore, snapshot_hash, Edit, EditValue, Format, KeyPath, Patch, Scope, Transaction};
use serde_json::{json, Value};
use std::{fs, os::unix::fs::{DirBuilderExt, PermissionsExt}, path::{Path, PathBuf}};

struct Fixture(PathBuf);
impl Fixture {
    fn new(base: &Path, name: &str) -> Self {
        let path = base.join(name);
        fs::DirBuilder::new().recursive(true).mode(0o700).create(&path).unwrap();
        Self(path)
    }
    fn file(&self) -> PathBuf { self.0.join("client/settings.json") }
    fn journal(&self) -> PathBuf { self.0.join("state/journal.json") }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fn repair(path: &Path) {
            let _ = fs::set_permissions(path, fs::Permissions::from_mode(0o700));
            if let Ok(entries) = fs::read_dir(path) {
                for entry in entries.flatten() {
                    if entry.path().is_dir() { repair(&entry.path()); }
                }
            }
        }
        repair(&self.0);
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn write(path: &Path, bytes: &[u8]) {
    fs::DirBuilder::new().recursive(true).mode(0o700).create(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
}
fn patch(path: &Path, parts: &[&str], value: Value) -> Patch {
    let key = KeyPath::new(parts.iter().copied()).unwrap();
    Patch { path: path.to_owned(), expected_hash: snapshot_hash(fs::read(path).ok().as_deref()), format: Format::StrictJson, scope: Scope::UserPrivate, edits: vec![Edit::Set { key: key.clone(), value: EditValue::Public(value) }], owned_keys: vec![key], warnings: vec![] }
}
fn run(fixture: &Fixture, patch: Patch) -> Result<(), llmgw::config_patch::Error> {
    let tx = Transaction { journal_path: fixture.journal(), patches: vec![patch] };
    let shown = preview(&tx).unwrap();
    apply(&tx, &shown.hash).map(|_| ())
}
fn main() {
    let base = PathBuf::from(std::env::args_os().nth(1).unwrap());
    let mut out = Vec::new();
    {
        let fixture = Fixture::new(&base, "created-reconnect");
        let path = fixture.file();
        run(&fixture, patch(&path, &["model"], json!("gateway-v1"))).unwrap();
        write(&path, br#"{"model":"gateway-v1","user_added":"preserve-me"}"#);
        run(&fixture, patch(&path, &["model"], json!("gateway-v2"))).unwrap();
        let restored = restore(fixture.journal()).unwrap();
        let value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        out.push(json!({"case":"created_user_edit_reconnect_restore","file_exists":path.exists(),"user_value":value["user_added"],"model_absent":value.get("model").is_none(),"preserved_created_files":restored.preserved_created_files.len(),"spec_pass":path.exists() && value["user_added"]=="preserve-me" && value.get("model").is_none()}));
    }
    {
        let fixture = Fixture::new(&base, "overlap-reconnect");
        let path = fixture.file();
        let original = br#"{"provider":{"model":"original","other":"keep"}}"#;
        write(&path, original);
        run(&fixture, patch(&path, &["provider","model"], json!("gateway"))).unwrap();
        let before_rejected = fs::read(&path).unwrap();
        let second = patch(&path, &["provider"], json!({"model":"gateway","other":"changed"}));
        let rejected = run(&fixture, second).is_err();
        let unchanged = fs::read(&path).unwrap() == before_rejected;
        restore(fixture.journal()).unwrap();
        let final_value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        out.push(json!({"case":"reconnect_overlap_refused","second_rejected":rejected,"bytes_unchanged_before_restore":unchanged,"final_value":final_value,"journal_status":format!("{:?}",inspect_journal(fixture.journal()).unwrap().status),"spec_pass":rejected && unchanged && final_value["provider"]["model"]=="original"}));
    }
    {
        let fixture = Fixture::new(&base, "unstarted-resource");
        let files = (0..3).map(|index| fixture.0.join(format!("client-{index}/settings.json"))).collect::<Vec<_>>();
        for path in &files { write(path, br#"{"model":"original"}"#); }
        fs::set_permissions(files[1].parent().unwrap(), fs::Permissions::from_mode(0o500)).unwrap();
        let tx = Transaction { journal_path: fixture.journal(), patches: files.iter().map(|path| patch(path, &["model"], json!("gateway"))).collect() };
        let shown = preview(&tx).unwrap();
        let failed = apply(&tx, &shown.hash).is_err();
        let journal = inspect_journal(fixture.journal()).unwrap();
        let stages = journal.resources.iter().map(|resource| resource.stage.as_str()).collect::<Vec<_>>();
        let counts = journal.resources.iter().map(|resource| resource.owned_keys.len()).collect::<Vec<_>>();
        out.push(json!({"case":"three_resources_second_fails","apply_failed":failed,"recorded_resources":journal.resources.len(),"stages":stages,"owned_key_counts":counts,"third_unchanged":fs::read(&files[2]).unwrap()==br#"{"model":"original"}"#,"spec_pass":failed && journal.resources.len()==3 && stages==["verified","failed","unstarted"] && counts==[1,0,0]}));
    }
    println!("{}", serde_json::to_string_pretty(&out).unwrap());
}
