use llmgw::config_patch::{apply,preview,restore,snapshot_hash,cooperating_lock_path,Edit,EditValue,Format,KeyPath,Patch,Scope,Transaction};
use serde_json::{json,Value};
use std::{fs,path::{Path,PathBuf},os::unix::fs::PermissionsExt};

struct Fixture(PathBuf);
impl Fixture {
    fn new(label:&str)->Self {
        let p=std::env::temp_dir().join(format!("quality-{label}-{}",std::process::id()));
        fs::create_dir(&p).unwrap();fs::set_permissions(&p,fs::Permissions::from_mode(0o700)).unwrap();Self(p)
    }
    fn file(&self,name:&str)->PathBuf {self.0.join(name)}
}
impl Drop for Fixture {fn drop(&mut self){fs::remove_dir_all(&self.0).unwrap();}}
fn write(p:&Path,b:&[u8]) {fs::write(p,b).unwrap();fs::set_permissions(p,fs::Permissions::from_mode(0o600)).unwrap();}
fn tx(f:&Fixture,p:&Path,format:Format,entries:&[(&[&str],Value)])->Transaction {
    let edits:Vec<_>=entries.iter().map(|(k,v)|Edit::Set{key:KeyPath::new(k.iter().copied()).unwrap(),value:EditValue::Public(v.clone())}).collect();
    Transaction{journal_path:f.file("state/journal.json"),patches:vec![Patch{path:p.into(),format,scope:Scope::UserPrivate,expected_hash:snapshot_hash(fs::read(p).ok().as_deref()),owned_keys:edits.iter().map(|e|e.key().clone()).collect(),edits,warnings:vec![]}]}
}
fn connect(t:&Transaction) {let p=preview(t).unwrap();apply(t,&p.hash).unwrap();}

#[test]
fn absent_case_alias_must_honor_existing_cooperating_lock() {
    use fs4::FileExt;
    let f=Fixture::new("absent-case");let marker=f.file("probe");write(&marker,b"marker");
    if !f.file("PROBE").exists(){return;}
    let upper=f.file("SETTINGS.JSON");let lower=f.file("settings.json");
    let lockpath=cooperating_lock_path(&upper).unwrap();
    let file=fs::OpenOptions::new().read(true).write(true).create_new(true).open(&lockpath).unwrap();
    fs::set_permissions(&lockpath,fs::Permissions::from_mode(0o600)).unwrap();FileExt::lock(&file).unwrap();
    let t=tx(&f,&lower,Format::StrictJson,&[(&["model"],json!("gateway"))]);
    let shown=preview(&t).unwrap();let result=apply(&t,&shown.hash);
    let different_lock=lockpath!=cooperating_lock_path(&lower).unwrap();
    let wrote=lower.exists();drop(file);
    assert!(result.is_err() && !wrote,"cooperating lock bypass: different_lock={different_lock}, apply_succeeded={}, target_written={wrote}",result.is_ok());
}

#[test]
fn toml_datetime_owned_value_is_refused_before_writes() {
    let f=Fixture::new("toml-date");let p=f.file("settings.toml");
    write(&p,b"last_used = 2026-09-13T07:00:00Z\nkeep = 42\n");
    let before=fs::read(&p).unwrap();
    let t=tx(&f,&p,Format::Toml,&[(&["last_used"],json!("gateway"))]);
    let error=preview(&t).unwrap_err().to_string();
    assert!(error.contains("native date or time"));assert!(!error.contains("2026-09-13"));
    assert!(apply(&t,"unreviewed").is_err());
    assert_eq!(fs::read(&p).unwrap(),before);assert!(!t.journal_path.exists());assert!(!f.file("state").exists());

}

#[test]
fn partial_restore_reconnect_disjoint_and_same_key_keep_first_values() {
    let f=Fixture::new("partial-connect");let p=f.file("settings.json");
    write(&p,br#"{"model":"original","url":"original-url","other":5}"#);
    let t=tx(&f,&p,Format::StrictJson,&[(&["model"],json!("gw1")),(&["url"],json!("gw-url"))]);connect(&t);
    write(&p,br#"{"model":"user-change","url":"gw-url","other":6}"#);
    let report=restore(&t.journal_path).unwrap();assert_eq!(report.conflicts.len(),1);
    let next=tx(&f,&p,Format::StrictJson,&[(&["model"],json!("gw2")),(&["extra"],json!(true))]);connect(&next);
    assert!(restore(&next.journal_path).unwrap().conflicts.is_empty());
    let value:Value=serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();
    assert_eq!(value,json!({"model":"original","url":"original-url","other":6}));
    assert_eq!(restore(&next.journal_path).unwrap().restored_resources,0);
}

#[test]
fn escaped_key_names_and_nested_array_original_restore() {
    let f=Fixture::new("escaped-key");let p=f.file("settings.jsonc");
    let initial=br#"{/* untouched */ "a.b":{"q\"x":[{"n":1},[true,null]]},"keep":"//data"}"#;
    write(&p,initial);
    let t=tx(&f,&p,Format::JsonWithComments,&[(&["a.b","q\"x"],json!([2,3]))]);connect(&t);
    assert!(restore(&t.journal_path).unwrap().conflicts.is_empty());
    let value=fs::read_to_string(&p).unwrap();assert!(value.contains("/* untouched */"));
    let jsontext=value.replace("/* untouched */","");let parsed:Value=serde_json::from_str(&jsontext).unwrap();
    assert_eq!(parsed,json!({"a.b":{"q\"x":[{"n":1},[true,null]]},"keep":"//data"}));
}

#[test]
fn unicode_alias_lock_persists_through_create_restore_recreate() {
    use fs4::FileExt;
    use std::os::unix::fs::MetadataExt;
    let f=Fixture::new("unicode-transition");
    let p=f.file("caf\u{e9}.json");let alias=f.file("cafe\u{301}.json");
    write(&p,b"{}");if !alias.exists(){return;}fs::remove_file(&p).unwrap();
    let lockpath=cooperating_lock_path(&p).unwrap();
    let lock=fs::OpenOptions::new().read(true).write(true).create_new(true).open(&lockpath).unwrap();
    fs::set_permissions(&lockpath,fs::Permissions::from_mode(0o600)).unwrap();
    FileExt::lock(&lock).unwrap();
    let t=tx(&f,&alias,Format::StrictJson,&[(&["model"],json!("gw"))]);
    let reviewed=preview(&t).unwrap();
    assert!(apply(&t,&reviewed.hash).unwrap_err().to_string().contains("another llmgw client resource edit"));
    assert!(!p.exists());let inode=lock.metadata().unwrap().ino();drop(lock);
    connect(&t);assert_eq!(fs::metadata(&lockpath).unwrap().ino(),inode);
    let lock=fs::OpenOptions::new().read(true).write(true).open(&lockpath).unwrap();FileExt::lock(&lock).unwrap();
    assert!(restore(&t.journal_path).unwrap_err().to_string().contains("another llmgw client resource edit"));
    assert!(p.exists());drop(lock);
    assert!(restore(&t.journal_path).unwrap().conflicts.is_empty());assert!(!p.exists());
    assert_eq!(fs::metadata(&lockpath).unwrap().ino(),inode);
    let t=tx(&f,&p,Format::StrictJson,&[(&["model"],json!("again"))]);connect(&t);restore(&t.journal_path).unwrap();
    assert_eq!(fs::metadata(&lockpath).unwrap().ino(),inode);assert!(!p.exists());
}

#[test]
fn toml_owned_array_of_tables_temporal_remove_is_refused() {
    let f=Fixture::new("temporal-aot");let p=f.file("settings.toml");
    let bytes=b"keep = true\n[[events]]\ntime = 07:32:00\n[[events]]\ntime = 1979-05-27\n";write(&p,bytes);
    let mut t=tx(&f,&p,Format::Toml,&[(&["events"],json!("unused"))]);
    t.patches[0].edits=vec![Edit::Remove{key:KeyPath::new(["events"]).unwrap()}];
    assert!(preview(&t).unwrap_err().to_string().contains("native date or time"));
    assert!(apply(&t,"no-review").is_err());assert_eq!(fs::read(&p).unwrap(),bytes);assert!(!f.file("state").exists());
}

#[test]
fn toml_unrelated_temporal_sibling_and_date_shaped_marker_table_restore() {
    let f=Fixture::new("temporal-sibling");let p=f.file("settings.toml");
    let bytes=b"[profile]\nat = 1979-05-27T07:32:00\nmodel = \"original\"\nmarker = { \"$__toml_private_datetime\" = \"2026-09-13T07:00:00Z\" }\n";
    write(&p,bytes);let t=tx(&f,&p,Format::Toml,&[(&["profile","model"],json!("gateway")),(&["profile","marker"],json!({"changed":true}))]);
    connect(&t);let report=restore(&t.journal_path).unwrap();assert!(report.conflicts.is_empty());
    let text=fs::read_to_string(&p).unwrap();let doc=text.parse::<toml_edit::DocumentMut>().unwrap();
    assert!(doc["profile"]["at"].as_datetime().is_some());
    assert_eq!(doc["profile"]["model"].as_str(),Some("original"));
    assert_eq!(doc["profile"]["marker"].as_inline_table().unwrap().get("$__toml_private_datetime").unwrap().as_str(),Some("2026-09-13T07:00:00Z"));
}

#[test]
fn nonfinite_owned_toml_is_refused_or_roundtrips() {
    let f=Fixture::new("nonfinite");let p=f.file("settings.toml");
    write(&p,b"value = nan\n");let t=tx(&f,&p,Format::Toml,&[(&["value"],json!(1))]);
    match preview(&t) {
        Err(_) => {assert_eq!(fs::read(&p).unwrap(),b"value = nan\n");assert!(!f.file("state").exists());}
        Ok(reviewed) => {apply(&t,&reviewed.hash).unwrap();restore(&t.journal_path).expect("accepted owned TOML value must safely restore");
            let doc=fs::read_to_string(&p).unwrap().parse::<toml_edit::DocumentMut>().unwrap();assert!(doc["value"].as_float().unwrap().is_nan());}
    }
}
