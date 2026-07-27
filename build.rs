use std::{env, fs, path::PathBuf, process::Command};

fn main() {
    let out = PathBuf::from(env::var_os("OUT_DIR").expect("cargo sets OUT_DIR"));
    let interfaces = out.join("ui");
    fs::create_dir_all(&interfaces).expect("a directory for the interfaces");
    let mut blueprints: Vec<PathBuf> = fs::read_dir("data/ui")
        .expect("the blueprints")
        .map(|entry| entry.expect("a blueprint").path())
        .filter(|path| path.extension().is_some_and(|extension| extension == "blp"))
        .collect();
    blueprints.sort();
    let status = Command::new("blueprint-compiler")
        .arg("batch-compile")
        .arg(&interfaces)
        .arg("data/ui")
        .args(&blueprints)
        .status()
        .expect("blueprint-compiler not found; install it (dnf install blueprint-compiler)");
    assert!(status.success(), "the blueprints did not compile");

    glib_build_tools::compile_resources(
        &[out.as_path(), "data".as_ref()],
        "data/chord.gresource.xml",
        "chord.gresource",
    );

    println!("cargo::rerun-if-changed=data/ui");
    println!("cargo::rerun-if-changed=data/chord.gresource.xml");
}
