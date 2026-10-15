use std::{env, fs, path::PathBuf};

fn main() {
    let manifest =
        PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("Cargo manifest directory"))
            .join("Cargo.toml");
    println!("cargo:rerun-if-changed={}", manifest.display());
    let manifest: toml::Value = fs::read_to_string(manifest)
        .expect("read server manifest")
        .parse()
        .expect("parse server manifest");
    let dependency = &manifest["dependencies"]["eoka"];
    let requirement = dependency
        .as_str()
        .or_else(|| dependency.get("version").and_then(toml::Value::as_str))
        .expect("server eoka dependency version");
    let version = requirement
        .strip_prefix('=')
        .map(str::trim)
        .filter(|version| !version.is_empty() && !version.contains(',') && !version.contains(' '))
        .expect("server eoka dependency must use one exact version for honest runtime metadata");
    println!("cargo:rustc-env=EOKA_CORE_VERSION={version}");
}
