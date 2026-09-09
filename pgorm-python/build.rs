use std::{env, fs, path::PathBuf};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?);
    let manifest_path = root.join("../Cargo.toml");
    let manifest: toml::Table = toml::from_str(&fs::read_to_string(&manifest_path)?)?;
    let version = manifest["package"]["version"]
        .as_str()
        .ok_or("pgorm package version must be a string")?;
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rustc-env=PGORM_VERSION={version}");
    println!(
        "cargo:rustc-env=PGORM_BINDING_TARGET={}",
        env::var("TARGET")?
    );
    Ok(())
}
