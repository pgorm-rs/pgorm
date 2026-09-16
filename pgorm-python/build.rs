use std::{env, error::Error, fs, path::Path};

/// The dependency patches this crate has to repeat from the workspace root.
///
/// `pgorm-python` is its own workspace, so Cargo reads the crates-io patch
/// table from this manifest and never from the root's. A patch the root pins
/// and this crate omits is resolved past in silence: the extension links the
/// unpatched crate and behaves differently from every pgorm build that is
/// tested. The two tables are compared here rather than trusted, because the
/// symptom of a drift is wrong SQL rather than a build error.
///
/// The header is matched as a whole line, so prose naming the table — this
/// file, and the manifests' own comments — is not mistaken for it.
// [spec:pgorm:req:python.optional]
fn patches(manifest: &str) -> Vec<String> {
    manifest
        .lines()
        .skip_while(|line| line.trim() != "[patch.crates-io]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_owned)
        .collect()
}

/// The two manifests' patch tables, or the difference between them.
// [spec:pgorm:req:python.optional]
fn agreeing_patches(workspace: &str, own: &str) -> Result<(), String> {
    let (wanted, held) = (patches(workspace), patches(own));
    if wanted == held {
        return Ok(());
    }
    Err(format!(
        "pgorm-python is a separate workspace and resolves [patch.crates-io] \
         from its own manifest, so it must repeat the workspace root's table \
         verbatim.\n  root:        {wanted:?}\n  pgorm-python: {held:?}"
    ))
}

fn main() -> Result<(), Box<dyn Error>> {
    let root = Path::new(&env::var("CARGO_MANIFEST_DIR")?).to_path_buf();
    let manifest_path = root.join("../Cargo.toml");
    let workspace = fs::read_to_string(&manifest_path)?;
    let manifest: toml::Table = toml::from_str(&workspace)?;
    let version = manifest["package"]["version"]
        .as_str()
        .ok_or("pgorm package version must be a string")?;
    let own_path = root.join("Cargo.toml");
    agreeing_patches(&workspace, &fs::read_to_string(&own_path)?)?;
    println!("cargo:rerun-if-changed={}", manifest_path.display());
    println!("cargo:rerun-if-changed={}", own_path.display());
    println!("cargo:rustc-env=PGORM_VERSION={version}");
    println!(
        "cargo:rustc-env=PGORM_BINDING_TARGET={}",
        env::var("TARGET")?
    );
    Ok(())
}
