use std::{fs, path::PathBuf};
use std::os::unix::fs::{DirBuilderExt,OpenOptionsExt,PermissionsExt};
use llmgw::config_patch::{cooperating_lock_path,preview,apply,Edit,EditValue,Format,KeyPath,Patch,Scope,Transaction,snapshot_hash};
fn main(){
 let root=PathBuf::from(std::env::args_os().nth(1).unwrap());
 let actual=root.join("client/settings.json");
 if std::env::args().nth(2).as_deref()==Some("key") { println!("{}",cooperating_lock_path(&actual).display()); return; }
 fs::DirBuilder::new().recursive(true).mode(0o700).create(actual.parent().unwrap()).unwrap();
 let before=br#"{"model":"original"}"#;
 fs::write(&actual,before).unwrap();fs::set_permissions(&actual,fs::Permissions::from_mode(0o600)).unwrap();
 let alias=root.join("client/../client/settings.json");
 let key_a=cooperating_lock_path(&actual);let key_b=cooperating_lock_path(&alias);
 fs::DirBuilder::new().recursive(true).mode(0o700).create(key_a.parent().unwrap()).unwrap();
 let lock=fs::OpenOptions::new().read(true).write(true).create_new(true).mode(0o600).open(&key_a).unwrap();
 fs4::FileExt::lock(&lock).unwrap();
 let key=KeyPath::new(["model"]).unwrap();
 let tx=Transaction{journal_path:root.join("state/journal.json"),patches:vec![Patch{path:alias.clone(),expected_hash:snapshot_hash(Some(before)),format:Format::StrictJson,scope:Scope::UserPrivate,edits:vec![Edit::Set{key:key.clone(),value:EditValue::Public("gateway".into())}],owned_keys:vec![key],warnings:vec![]}]};
 let reviewed=preview(&tx).unwrap();let result=apply(&tx,&reviewed.hash);
 println!("{{\"same_physical_file\":{},\"same_lock_path\":{},\"apply_succeeded_while_other_alias_locked\":{},\"file_changed\":{}}}",fs::canonicalize(&actual).unwrap()==fs::canonicalize(&alias).unwrap(),key_a==key_b,result.is_ok(),fs::read(&actual).unwrap()!=before);
 drop(lock);
}