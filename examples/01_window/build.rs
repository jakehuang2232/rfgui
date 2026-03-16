use std::env;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=../assets");

    let output_dir = target_profile_dir();
    let source_dir = Path::new("../assets");
    let target_dir = output_dir.join("assets");

    if let Err(error) = copy_dir_all(source_dir, &target_dir) {
        panic!(
            "failed to copy assets from {} to {}: {}",
            source_dir.display(),
            target_dir.display(),
            error
        );
    }
}

fn target_profile_dir() -> PathBuf {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").expect("OUT_DIR is not set"));
    let profile = env::var("PROFILE").expect("PROFILE is not set");

    let mut current = out_dir.as_path();
    loop {
        if current.file_name().and_then(|name| name.to_str()) == Some(profile.as_str()) {
            return current.to_path_buf();
        }
        current = current.parent().unwrap_or_else(|| {
            panic!(
                "failed to locate target profile directory from {}",
                out_dir.display()
            )
        });
    }
}

fn copy_dir_all(src: &Path, dst: &Path) -> io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let destination = dst.join(entry.file_name());
        if file_type.is_dir() {
            copy_dir_all(&entry.path(), &destination)?;
        } else {
            fs::copy(entry.path(), destination)?;
        }
    }
    Ok(())
}
