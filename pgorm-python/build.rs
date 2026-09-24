use std::{env, error::Error, fs, path::Path};

fn main() -> Result<(), Box<dyn Error>> {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR")?).to_path_buf();
    let manifest_path = root.join("../Cargo.toml");
    let workspace = fs::read_to_string(&manifest_path)?;
    let manifest: toml::Table = toml::from_str(&workspace)?;
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
