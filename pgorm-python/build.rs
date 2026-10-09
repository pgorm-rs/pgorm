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
    // The capability manifest names the PyO3 release the binding is built
    // with. Cargo.toml asks only for a compatible one, so the release is the
    // one the binding's lockfile holds: the lockfile its wheel and source
    // distribution build from.
    let lock_path = root.join("Cargo.lock");
    let lock: toml::Table = toml::from_str(&fs::read_to_string(&lock_path)?)?;
    let pyo3 = lock["package"]
        .as_array()
        .ok_or("Cargo.lock lists no packages")?
        .iter()
        .find(|package| package.get("name").and_then(toml::Value::as_str) == Some("pyo3"))
        .and_then(|package| package.get("version"))
        .and_then(toml::Value::as_str)
        .ok_or("Cargo.lock holds no pyo3 release")?;
    println!("cargo:rerun-if-changed={}", lock_path.display());
    println!("cargo:rustc-env=PGORM_PYO3_VERSION={pyo3}");
    Ok(())
}
