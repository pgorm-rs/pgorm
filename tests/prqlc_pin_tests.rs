//! Every crate that links prqlc has to link the one the workspace pins.
//!
//! Several crates here declare their own `[workspace]` — `pgorm-python` so
//! that Python configuration stays out of ordinary Rust builds
//! ([spec:pgorm:req:python.optional]), the campaign's bridge, replay harness,
//! sqlmap adapter and standalone reproducers so that their dependencies stay
//! out of an ordinary pgorm build. A `[patch.crates-io]` table is read from a
//! workspace root and nowhere else, so each of those separations silently
//! drops the root's prqlc patch unless the manifest repeats it, and the crate
//! then compiles a different SQL generator from the one every pgorm test
//! exercises.
//!
//! That is not hypothetical. It hid three fork commits from every campaign
//! run of 2026-09-15, so a full profile reported defects that were already
//! fixed and a fix under test was neither confirmed nor refuted.
//!
//! The first version of this file listed the two detached workspaces that
//! existed when it was written, which made it a check on two instances
//! wearing the shape of a check on the class. Eight more detached workspaces
//! were added afterwards, not one of them repeated the patch, and these tests
//! stayed green throughout. So the set is discovered here instead: every
//! manifest in the checkout that declares a workspace and reaches prqlc
//! through its dependencies. A crate deliberately left on stock prqlc has to
//! name itself in `EXEMPT` below, so an exemption is a decision someone can
//! read rather than an omission nobody can see.
//!
//! No database: every check reads version-controlled files.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// A detached crate deliberately left on stock prqlc.
///
/// Empty, and the empty case is the healthy one: every crate that resolves
/// its own patch table repeats the root's pin. The slot exists because one
/// crate legitimately wanted the opposite — `pipeline-join-distinct-order`
/// reproduces a prqlc defect the fork fixes, so pinning it would have made
/// the reproducer pass and destroyed the evidence. That stopped being true
/// when pgorm started binding a deduplicated join into a CTE: the reproducer
/// no longer distinguishes the two compilers, measured over five runs of
/// forty compilations on each, so it is pinned with the rest. Should another
/// reproducer need stock prqlc, it belongs here with its reason, and
/// `an_exempt_crate_stays_detached_and_unpatched` will hold the entry to it.
const EXEMPT: [&str; 0] = [];

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

/// One dependency edge as a manifest declares it.
#[derive(Debug)]
struct Dependency {
    name: String,
    /// The directory a path dependency names, relative to the manifest.
    path: Option<String>,
    /// Development dependencies are linked by the declaring crate's own
    /// tests, and by nothing that depends on it.
    development: bool,
}

/// Whether a section header names a dependency table, and whether that table
/// is the development one.
///
/// `[dependencies]`, `[build-dependencies]`, `[workspace.dependencies]` and
/// the platform-conditional `[target.'cfg(..)'.dependencies]` forms all
/// declare one dependency per line. The `[dependencies.name]` form declares
/// one across several, and is not read here — `no_manifest_hides_a_dependency`
/// holds the checkout to the form this reads.
fn dependency_table(header: &str) -> Option<bool> {
    let table = header.rsplit('.').next().unwrap_or(header);
    match table {
        "dependencies" | "build-dependencies" => Some(false),
        "dev-dependencies" => Some(true),
        _ => None,
    }
}

/// Every dependency a manifest declares, in the one-per-line form.
fn declared_dependencies(manifest: &str) -> Vec<Dependency> {
    let mut development = None;
    let mut declared = Vec::new();
    for line in manifest.lines() {
        let line = line.trim();
        if let Some(header) = line
            .strip_prefix('[')
            .and_then(|rest| rest.strip_suffix(']'))
        {
            development = dependency_table(header);
            continue;
        }
        let Some(development) = development else {
            continue;
        };
        if line.starts_with('#') {
            continue;
        }
        let Some((name, value)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim().trim_matches('"');
        let (name, _) = name.split_once('.').unwrap_or((name, ""));
        if name.is_empty() {
            continue;
        }
        declared.push(Dependency {
            name: name.to_owned(),
            path: quoted_after(value, "path"),
            development,
        });
    }
    declared
}

/// Whether one manifest's dependencies reach prqlc.
///
/// `carriers` are the manifests already known to carry prqlc into anything
/// that depends on them. `own` asks the question for the crate itself, which
/// links its development dependencies too.
fn reaches_prqlc(
    manifest_path: &Path,
    manifest: &str,
    carriers: &BTreeSet<PathBuf>,
    own: bool,
) -> bool {
    let directory = manifest_path.parent().unwrap_or(Path::new("."));
    declared_dependencies(manifest).iter().any(|dependency| {
        if dependency.development && !own {
            return false;
        }
        if dependency.name == "prqlc" {
            return true;
        }
        dependency.path.as_ref().is_some_and(|relative| {
            let target = directory.join(relative).join("Cargo.toml");
            std::fs::canonicalize(target).is_ok_and(|resolved| carriers.contains(&resolved))
        })
    })
}

/// The manifests whose compiled dependency graph contains prqlc.
///
/// A crate reaches prqlc by naming it or through a path dependency on a crate
/// that does, so the carrying set is grown to a fixed point first and each
/// manifest then asked about its own graph. The distinction is what keeps
/// `security/generative/compile` out: it depends on `pgorm-codegen`, whose
/// only edge to `pgorm` is a development dependency, and a development
/// dependency is linked by its declarer's tests and by nothing downstream.
fn linking_prqlc(manifests: &BTreeMap<PathBuf, String>) -> BTreeSet<PathBuf> {
    let mut carriers = BTreeSet::new();
    let mut growing = true;
    while growing {
        growing = false;
        for (path, manifest) in manifests {
            if !carriers.contains(path) && reaches_prqlc(path, manifest, &carriers, false) {
                carriers.insert(path.clone());
                growing = true;
            }
        }
    }
    manifests
        .iter()
        .filter(|(path, manifest)| reaches_prqlc(path, manifest, &carriers, true))
        .map(|(path, _)| path.clone())
        .collect()
}

/// Every `Cargo.toml` in the checkout.
///
/// `target/` holds an unpacked manifest for every registry dependency and is
/// a build output besides, so it is skipped rather than walked; so is any
/// dotted directory, `.git` above all.
fn manifests(root: &Path) -> BTreeMap<PathBuf, String> {
    let mut found = BTreeMap::new();
    let mut pending = vec![root.to_owned()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if name != "target" && !name.starts_with('.') {
                    pending.push(path);
                }
            } else if name == "Cargo.toml" {
                let canonical = std::fs::canonicalize(&path).unwrap_or(path);
                let text = read(&canonical);
                found.insert(canonical, text);
            }
        }
    }
    found
}

/// The crates that resolve `[patch.crates-io]` from their own manifest and
/// link prqlc, as paths relative to the checkout root.
///
/// Discovered rather than listed: a list covers the cases its author had
/// already noticed, which is how eight unpatched workspaces accumulated
/// under a test that claimed to cover them.
fn detached_crates_linking_prqlc(root: &Path) -> Vec<String> {
    let root = std::fs::canonicalize(root).unwrap_or_else(|_| root.to_owned());
    let manifests = manifests(&root);
    let linking = linking_prqlc(&manifests);
    let root_manifest = root.join("Cargo.toml");
    manifests
        .iter()
        .filter(|(path, manifest)| {
            **path != root_manifest
                && linking.contains(*path)
                && manifest.lines().any(|line| line.trim() == "[workspace]")
        })
        .filter_map(|(path, _)| path.parent()?.strip_prefix(&root).ok())
        .map(|relative| relative.to_string_lossy().into_owned())
        .collect()
}

// [spec:pgorm:req:python.optional/test]    a separate workspace carries the
// root's dependency patches rather than resolving past them
#[test]
fn every_detached_workspace_repeats_the_patch() {
    let root = repository_root();
    let workspace = read(&root.join("Cargo.toml"));
    let pinned = patched_prqlc(&workspace).expect("the workspace manifest patches prqlc");
    for crate_dir in detached_crates_linking_prqlc(&root) {
        if EXEMPT.contains(&crate_dir.as_str()) {
            continue;
        }
        let manifest = read(&root.join(&crate_dir).join("Cargo.toml"));
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
fn every_detached_lockfile_resolves_the_pinned_prqlc() {
    let root = repository_root();
    let workspace = read(&root.join("Cargo.toml"));
    let (url, revision) = patched_prqlc(&workspace).expect("the workspace manifest patches prqlc");
    let stripped = url.strip_suffix(".git").unwrap_or(&url);
    for crate_dir in detached_crates_linking_prqlc(&root) {
        if EXEMPT.contains(&crate_dir.as_str()) {
            continue;
        }
        let lock = read(&root.join(&crate_dir).join("Cargo.lock"));
        let source = locked_source(&lock, "prqlc")
            .unwrap_or_else(|| panic!("{crate_dir} links prqlc, so its lockfile resolves it"));
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

/// The discovery has to reach past the two crates the listed version knew
/// about, or it is the same test with more machinery.
#[test]
fn discovery_finds_the_crates_a_list_missed() {
    let found = detached_crates_linking_prqlc(&repository_root());
    for crate_dir in [
        "pgorm-python",
        "security/generative/bridge",
        "security/sqlmap/adapter",
        "security/generative/replay",
        "pgorm-python/tests/application-binding",
        "pgorm-python/tests/codegen-entities",
        "security/generative/findings/set-precedence",
    ] {
        assert!(
            found.iter().any(|entry| entry == crate_dir),
            "{crate_dir} declares its own workspace and links prqlc, so discovery must find it; \
             it found {found:?}"
        );
    }
    assert!(
        !found
            .iter()
            .any(|entry| entry == "security/generative/compile"),
        "security/generative/compile reaches pgorm only through pgorm-codegen's \
         development dependency, so it compiles no prqlc to patch"
    );
}

/// An exemption is a decision, so it has to keep describing a real crate:
/// one that still resolves its own patch table, still links prqlc, and is
/// still unpatched. Pinning an exempt crate without removing it from `EXEMPT`
/// would otherwise leave a reason behind that no longer explains anything.
#[test]
fn an_exempt_crate_stays_detached_and_unpatched() {
    let root = repository_root();
    let detached = detached_crates_linking_prqlc(&root);
    for crate_dir in EXEMPT {
        assert!(
            detached.iter().any(|entry| entry == crate_dir),
            "{crate_dir} is exempted from the prqlc pin but is not a detached \
             workspace linking prqlc, so the exemption has nothing to except"
        );
        let manifest = read(&root.join(crate_dir).join("Cargo.toml"));
        assert_eq!(
            patched_prqlc(&manifest),
            None,
            "{crate_dir} carries the prqlc patch and is also listed as exempt \
             from it; drop the EXEMPT entry, or the patch"
        );
    }
}

/// The dependency reader takes one dependency per line, which is how every
/// manifest here spells them. The `[dependencies.name]` table form would be
/// invisible to it, and a dependency the reader cannot see is a crate the
/// discovery above cannot reach.
#[test]
fn no_manifest_hides_a_dependency() {
    for (path, manifest) in manifests(&repository_root()) {
        for line in manifest.lines().map(str::trim) {
            let Some(header) = line
                .strip_prefix('[')
                .and_then(|rest| rest.strip_suffix(']'))
            else {
                continue;
            };
            let sections: Vec<_> = header.split('.').collect();
            let tail = sections.split_last().map(|(_, rest)| rest).unwrap_or(&[]);
            assert!(
                !tail
                    .iter()
                    .any(|section| dependency_table(section).is_some()),
                "{} declares {line} as its own table; tests/prqlc_pin_tests.rs reads \
                 one dependency per line and would not see it",
                path.display()
            );
        }
    }
}
