use llmgw::config_patch::{apply,preview,restore,inspect_journal,snapshot_hash,Edit,EditValue,Format,KeyPath,Patch,Scope,Transaction};
use serde_json::{json,Value};
use std::{fs,path::{Path,PathBuf},os::unix::fs::{PermissionsExt,DirBuilderExt}};
struct Fixture(PathBuf);
impl Fixture { fn new(base:&Path,name:&str)->Self { let p=base.join(name); fs::DirBuilder::new().recursive(true).mode(0o700).create(&p).unwrap();Self(p) } fn file(&self)->PathBuf{self.0.join("client/settings.json")} fn journal(&self)->PathBuf{self.0.join("state/journal.json")} }
impl Drop for Fixture{fn drop(&mut self){ fn fix(p:&Path){let _=fs::set_permissions(p,fs::Permissions::from_mode(0o700));if let Ok(es)=fs::read_dir(p){for e in es.flatten(){if e.path().is_dir(){fix(&e.path());}}}}fix(&self.0);let _=fs::remove_dir_all(&self.0);}}
fn write(p:&Path,b:&[u8]){fs::DirBuilder::new().recursive(true).mode(0o700).create(p.parent().unwrap()).unwrap();fs::write(p,b).unwrap();fs::set_permissions(p,fs::Permissions::from_mode(0o600)).unwrap();}
fn patch(p:&Path,parts:&[&str],v:Value)->Patch{let key=KeyPath::new(parts.iter().copied()).unwrap();Patch{path:p.to_owned(),expected_hash:snapshot_hash(fs::read(p).ok().as_deref()),format:Format::StrictJson,scope:Scope::UserPrivate,edits:vec![Edit::Set{key:key.clone(),value:EditValue::Public(v)}],owned_keys:vec![key],warnings:vec![]}}
fn run(f:&Fixture,p:Patch){let t=Transaction{journal_path:f.journal(),patches:vec![p]};let pv=preview(&t).unwrap();apply(&t,&pv.hash).unwrap();}
fn main(){let base=PathBuf::from(std::env::args_os().nth(1).unwrap());let mut out=Vec::new();
{
 let f=Fixture::new(&base,"created-reconnect");let p=f.file();run(&f,patch(&p,&["model"],json!("gateway-v1")));write(&p,br#"{"model":"gateway-v1","user_added":"preserve-me"}"#);run(&f,patch(&p,&["model"],json!("gateway-v2")));let before=fs::read(&p).unwrap();let r=restore(f.journal()).unwrap();out.push(json!({"case":"new_file_user_edit_reconnect_restore","unrelated_present_before_restore":String::from_utf8(before).unwrap().contains("preserve-me"),"file_exists_after_restore":p.exists(),"preserved_created_files":r.preserved_created_files.len(),"journal_status":format!("{:?}",inspect_journal(f.journal()).unwrap().status),"spec_pass":p.exists()}));
}
{
 let f=Fixture::new(&base,"overlapping-reconnect");let p=f.file();write(&p,br#"{"provider":{"model":"original","other":"keep"}}"#);run(&f,patch(&p,&["provider","model"],json!("gateway")));run(&f,patch(&p,&["provider"],json!({"model":"gateway","other":"changed"})));let r=restore(f.journal()).unwrap();let final_value:Value=serde_json::from_slice(&fs::read(&p).unwrap()).unwrap();out.push(json!({"case":"repeated_connect_descendant_then_ancestor","final_value":final_value,"conflicts":r.conflicts.len(),"journal_status":format!("{:?}",inspect_journal(f.journal()).unwrap().status),"spec_pass":final_value["provider"]["model"]=="original"}));
}
{
 let f=Fixture::new(&base,"unstarted-resource");let files=(0..3).map(|i|f.0.join(format!("client-{i}/settings.json"))).collect::<Vec<_>>();for p in &files{write(p,br#"{"model":"original"}"#);}fs::set_permissions(files[1].parent().unwrap(),fs::Permissions::from_mode(0o500)).unwrap();let t=Transaction{journal_path:f.journal(),patches:files.iter().map(|p|patch(p,&["model"],json!("gateway"))).collect()};let pv=preview(&t).unwrap();let result=apply(&t,&pv.hash);let j=inspect_journal(f.journal()).unwrap();out.push(json!({"case":"three_resources_second_fails","apply_failed":result.is_err(),"requested_resources":3,"recorded_resources":j.resources.len(),"recorded_owned_key_counts":j.resources.iter().map(|r|r.owned_keys.len()).collect::<Vec<_>>(),"third_unchanged":fs::read(&files[2]).unwrap()==br#"{"model":"original"}"#,"spec_pass":j.resources.len()==3}));
}
println!("{}",serde_json::to_string_pretty(&out).unwrap());}
