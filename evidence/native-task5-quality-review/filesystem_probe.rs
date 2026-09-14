use llmgw::autostart::{Action, Platform, RegistrationPlan};
use std::{fs, path::PathBuf};
use std::os::unix::fs::symlink;
struct Fixture(PathBuf);
impl Fixture {
    fn new(name: &str) -> Self {
        let p=PathBuf::from(std::env::var_os("REVIEW_OUT").unwrap()).join(name);
        fs::create_dir(&p).unwrap();
        fs::write(p.join("llmgw"), b"synthetic executable").unwrap();
        fs::write(p.join("gateway.toml"), b"synthetic config").unwrap();
        Self(p)
    }
    fn plan(&self) -> RegistrationPlan {
        RegistrationPlan::prepare(Platform::Macos,&self.0.join("llmgw"),&self.0.join("gateway.toml"),&self.0,Action::On).unwrap()
    }
}
impl Drop for Fixture {fn drop(&mut self){fs::remove_dir_all(&self.0).unwrap();}}
#[test]
fn dangling_temporary_symlink_must_be_refused() {
    let fixture=Fixture::new("owned-dangling-temp");
    let plan=fixture.plan();
    let parent=plan.target_path().parent().unwrap();fs::create_dir_all(parent).unwrap();
    let temp=parent.join(format!(".{}.{}.tmp",plan.spec().label(),std::process::id()));
    let foreign=fixture.0.join("unrelated-missing-file");
    symlink(&foreign,&temp).unwrap();
    let result=plan.apply();
    let redirected=foreign.exists();
    let target_symlink=fs::symlink_metadata(plan.target_path()).is_ok_and(|m|m.file_type().is_symlink());
    println!("dangling temporary: apply={result:?}, unrelated_file_created={redirected}, target_is_symlink={target_symlink}");
    assert!(result.is_err() && !redirected,"registration followed a dangling temporary symlink and wrote outside the reviewed target");
}
#[test]
fn regular_temporary_collision_is_preserved() {
    let fixture=Fixture::new("owned-regular-temp");let plan=fixture.plan();
    let parent=plan.target_path().parent().unwrap();fs::create_dir_all(parent).unwrap();
    let temp=parent.join(format!(".{}.{}.tmp",plan.spec().label(),std::process::id()));
    fs::write(&temp,b"synthetic unrelated contents").unwrap();
    assert!(plan.apply().is_err());assert_eq!(fs::read(&temp).unwrap(),b"synthetic unrelated contents");assert!(!plan.target_path().exists());
}
#[test]
fn changed_preview_preserves_foreign_target() {
    let fixture=Fixture::new("owned-changed-preview");let plan=fixture.plan();
    fs::create_dir_all(plan.target_path().parent().unwrap()).unwrap();
    fs::write(plan.target_path(),b"synthetic foreign target").unwrap();
    assert!(plan.apply().is_err());assert_eq!(fs::read(plan.target_path()).unwrap(),b"synthetic foreign target");
}
