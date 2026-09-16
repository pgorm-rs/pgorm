//! The Python extension has to link the same prqlc the workspace pins.
//!
//! `pgorm-python` declares its own `[workspace]` so that Python configuration
//! stays out of ordinary Rust builds ([spec:pgorm:req:python.optional]). A
//! `[patch.crates-io]` table is read from the workspace root and nowhere else,
//! so that separation silently drops the root's prqlc patch unless the Python
//! manifest carries it too — and the extension then compiles a different SQL
//! generator from the one every pgorm test exercises.
//!
//! No database: both checks read version-controlled files.

use std::path::{Path, PathBuf};

fn repository_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read(path: &Path) -> String {
    match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) => panic!("{} must be readable: {error}", path.display()),
    }
}

/// The text between the first pair of double quotes after `key =` in `line`.
fn quoted_after(line: &str, key: &str) -> Option<String> {
    let rest = line.split_once(key)?.1;
    let rest = rest.split_once('"')?.1;
    let (value, _) = rest.split_once('"')?;
    Some(value.to_owned())
}

/// The `prqlc` entry of a manifest's crates-io patch table, as the git URL
/// and revision it names.
///
/// The header is matched as a whole line, so prose naming the table is not
/// mistaken for it.
fn patched_prqlc(manifest: &str) -> Option<(String, String)> {
    let line = manifest
        .lines()
        .skip_while(|line| line.trim() != "[patch.crates-io]")
        .skip(1)
        .take_while(|line| !line.trim_start().starts_with('['))
        .find(|line| line.trim_start().starts_with("prqlc"))?;
    Some((quoted_after(line, "git")?, quoted_after(line, "rev")?))
}

/// The `source` a lockfile resolved `name` from, or `None` when the package
/// is absent or vendored by path.
fn locked_source(lock: &str, name: &str) -> Option<String> {
    lock.split("[[package]]")
        .find(|block| {
            block
                .lines()
                .any(|line| line.trim() == format!("name = \"{name}\""))
        })?
        .lines()
        .find(|line| line.trim_start().starts_with("source = "))
        .and_then(|line| quoted_after(line, "source"))
}

// [spec:pgorm:req:python.optional/test]    the separate Python workspace
// carries the workspace root's dependency patches rather than resolving past
// them
#[test]
fn the_python_manifest_repeats_the_workspace_patch() {
    let root = repository_root();
    let workspace = read(&root.join("Cargo.toml"));
    let python = read(&root.join("pgorm-python/Cargo.toml"));
    let pinned = patched_prqlc(&workspace).expect("the workspace manifest patches prqlc");
    assert_eq!(
        patched_prqlc(&python),
        Some(pinned),
        "pgorm-python is its own workspace, so it resolves [patch.crates-io] \
         from its own manifest; without the same prqlc entry the extension \
         links upstream prqlc and compiles different SQL"
    );
}

// [spec:pgorm:req:python.optional/test]    and its lockfile resolves the
// patched revision, which is what the campaign's `--locked` build reads
#[test]
fn the_python_lockfile_resolves_the_pinned_prqlc() {
    let root = repository_root();
    let workspace = read(&root.join("Cargo.toml"));
    let lock = read(&root.join("pgorm-python/Cargo.lock"));
    let (url, revision) = patched_prqlc(&workspace).expect("the workspace manifest patches prqlc");
    let source = locked_source(&lock, "prqlc").expect("pgorm-python locks prqlc");
    let stripped = url.strip_suffix(".git").unwrap_or(&url);
    assert!(
        source.starts_with("git+") && source.contains(stripped),
        "pgorm-python/Cargo.lock resolves prqlc from {source}, not from {url}"
    );
    assert!(
        source.contains(&revision),
        "pgorm-python/Cargo.lock resolves prqlc at {source}, not at {revision}"
    );
}
