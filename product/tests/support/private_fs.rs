use std::{io::Write, path::Path};

#[allow(dead_code)]
#[path = "../../src/lifecycle/platform/mod.rs"]
mod platform;

pub fn private_dir(path: &Path) {
    platform::directory_tree(path).unwrap();
}

pub fn write_private(path: &Path, bytes: &[u8]) {
    private_dir(path.parent().unwrap());
    let mut file = platform::open(path, true, false).unwrap();
    file.set_len(0).unwrap();
    file.write_all(bytes).unwrap();
}
