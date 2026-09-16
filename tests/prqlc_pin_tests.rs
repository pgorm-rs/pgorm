//! Every crate that links prqlc has to link the one the workspace pins.
//!
//! Several crates here declare their own `[workspace]` — `pgorm-python` so
//! that Python configuration stays out of ordinary Rust builds
//! ([spec:pgorm:req:python.optional]), the campaign's bridge so that its
//! PyO3 build does the same. A `[patch.crates-io]` table is read from the
//! workspace root and nowhere else, so each of those separations silently
//! drops the root's prqlc patch unless the manifest repeats it, and the crate
//! then compiles a different SQL generator from the one every pgorm test
//! exercises.
//!
//! That is not hypothetical. It hid three fork commits from every campaign
//! run of 2026-09-15, so a full profile reported defects that were already
//! fixed and a fix under test was neither confirmed nor refuted.
//!
//! No database: every check reads version-controlled files.

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

/// The crates that declare their own workspace *and* reach prqlc through
/// pgorm, so each needs the patch repeated and its lock refreshed.
const DETACHED: [&str; 2] = ["pgorm-python", "security/generative/bridge"];

// [spec:pgorm:req:python.optional/test]    a separate workspace carries the
// root's dependency patches rather than resolving past them
#[test]
fn a_detached_manifest_repeats_the_workspace_patch() {
    let root = repository_root();
    let workspace = read(&root.join("Cargo.toml"));
    let pinned = patched_prqlc(&workspace).expect("the workspace manifest patches prqlc");
    for crate_dir in DETACHED {
        let manifest = read(&root.join(crate_dir).join("Cargo.toml"));
        assert_eq!(
            patched_prqlc(&manifest),
            Some(pinned.clone()),
            "{crate_dir} is its own workspace, so it resolves [patch.crates-io] \
             from its own manifest; without the same prqlc entry it links \
             upstream prqlc and compiles different SQL"
        );
    }
}

// [spec:pgorm:req:python.optional/test]    and its lockfile resolves the
// patched revision, which is what a `--locked` build reads
#[test]
fn a_detached_lockfile_resolves_the_pinned_prqlc() {
    let root = repository_root();
    let workspace = read(&root.join("Cargo.toml"));
    let (url, revision) = patched_prqlc(&workspace).expect("the workspace manifest patches prqlc");
    let stripped = url.strip_suffix(".git").unwrap_or(&url);
    for crate_dir in DETACHED {
        let lock = read(&root.join(crate_dir).join("Cargo.lock"));
        let source =
            locked_source(&lock, "prqlc").unwrap_or_else(|| panic!("{crate_dir} locks prqlc"));
        assert!(
            source.starts_with("git+") && source.contains(stripped),
            "{crate_dir}/Cargo.lock resolves prqlc from {source}, not from {url}"
        );
        assert!(
            source.contains(&revision),
            "{crate_dir}/Cargo.lock resolves prqlc at {source}, not at {revision}"
        );
    }
}
