//! The spec cites a rule by its id alone.
//!
//! A rule's marker, `> [spec:pgorm:req:a.b+3]`, states the rule's version, and
//! code cites a rule at a version (`[spec:pgorm:req:a.b+3]`, or `+3/test` from
//! a test) so that a bump finds every site written against the old text. Prose
//! has no use for the number: a pin there goes stale at the cited rule's next
//! bump, and correcting it edits the citing rule's body, a change nspec's
//! convention answers with a bump of the citing rule too, so a stale number
//! tends to stay. Rule text and the prose around it therefore cite
//! `[spec:pgorm:req:a.b]`, or `a.b` in backticks, and these tests refuse a
//! pinned citation anywhere under `docs/spec` except a marker.
//!
//! No database: every check reads version-controlled files.

use std::path::{Path, PathBuf};

/// Every Markdown file under `docs/spec`, nested directories included.
fn spec_files() -> Vec<PathBuf> {
    fn walk(dir: &Path, files: &mut Vec<PathBuf>) {
        let entries = match std::fs::read_dir(dir) {
            Ok(entries) => entries,
            Err(error) => panic!("{} must be readable: {error}", dir.display()),
        };
        for entry in entries {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(error) => panic!("{} must be listable: {error}", dir.display()),
            };
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    walk(
        &Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/spec"),
        &mut files,
    );
    files.sort();
    files
}

/// Whether `line` is a rule's marker: a blockquote line holding one
/// `[spec:pgorm:…]` and nothing after it. A line of prose that happens to open
/// with a citation goes on, so it is not one.
fn is_marker(line: &str) -> bool {
    line.strip_prefix("> [spec:pgorm:")
        .and_then(|rest| rest.strip_suffix(']'))
        .is_some_and(|inside| !inside.contains([']', ' ']))
}

/// Whether `text`, a run of lowercase letters, digits, dots and hyphens, is
/// shaped as a rule id, `sql.ddl.create-table`: two or more non-empty
/// dot-separated parts, the first opening with a letter. That shape is what
/// tells a pin from the `+` of a timestamp's offset, an exponent or an
/// interval's field.
fn is_rule_id(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_lowercase())
        && text.contains('.')
        && text.split('.').all(|part| !part.is_empty())
}

/// The version-pinned rule citations in `line`, each as written, `a.b+3`:
/// inside `[spec:pgorm:…]`, in backticks, or bare. A run that continues a
/// longer word (`Value.x+1`, `a_b.c+2`) is not a citation.
fn pins(line: &str) -> Vec<&str> {
    let bytes = line.as_bytes();
    let mut found = Vec::new();
    for (plus, _) in line.match_indices('+') {
        let digits = bytes[plus + 1..]
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if digits == 0 {
            continue;
        }
        let start = bytes[..plus]
            .iter()
            .rposition(|&byte| {
                !(byte.is_ascii_lowercase()
                    || byte.is_ascii_digit()
                    || byte == b'.'
                    || byte == b'-')
            })
            .map_or(0, |boundary| boundary + 1);
        let follows_a_word = start > 0 && {
            let before = bytes[start - 1];
            before.is_ascii_alphanumeric() || before == b'_'
        };
        if !follows_a_word && is_rule_id(&line[start..plus]) {
            found.push(&line[start..=plus + digits]);
        }
    }
    found
}

/// A pin in prose would go stale at the cited rule's next bump. The message
/// names each one so that dropping its `+N` is the whole fix.
#[test]
fn the_spec_cites_no_rule_at_a_version() {
    let files = spec_files();
    assert!(!files.is_empty(), "docs/spec holds no Markdown file");
    let mut markers = 0;
    let mut pinned = Vec::new();
    for file in &files {
        let text = match std::fs::read_to_string(file) {
            Ok(text) => text,
            Err(error) => panic!("{} must be readable: {error}", file.display()),
        };
        for (index, line) in text.lines().enumerate() {
            if is_marker(line) {
                markers += 1;
                continue;
            }
            for pin in pins(line) {
                pinned.push(format!("{}:{}: {pin}", file.display(), index + 1));
            }
        }
    }
    assert!(markers > 0, "docs/spec defines no rule");
    assert!(
        pinned.is_empty(),
        "rule text cites these rules at a version; cite each by its id alone, without the +N:\n{}",
        pinned.join("\n")
    );
}

/// The scan is only as good as what it tells apart, so its cases are held
/// here rather than inferred from a spec that may happen to have none.
#[test]
fn a_pin_is_told_from_other_plus_signs() {
    assert!(is_marker("> [spec:pgorm:req:sql.ddl.view+1]"));
    assert!(is_marker("> [spec:pgorm:sem:sql.value.interval]"));
    assert!(!is_marker(
        "> [spec:pgorm:req:query.loader.table-ref-limitation+3] rather than rendering SQL"
    ));
    assert!(!is_marker("> see [spec:pgorm:req:sql.ddl.view+1]"));

    assert_eq!(
        pins("> (`[spec:pgorm:req:sql.ddl.create-table+16]`) and `exec.cursor.keyset+6`"),
        ["sql.ddl.create-table+16", "exec.cursor.keyset+6"]
    );
    assert_eq!(
        pins("> [spec:pgorm:req:query.loader.table-ref-limitation+3] rather than"),
        ["query.loader.table-ref-limitation+3"]
    );
    assert_eq!(
        pins("> as bare text, sql.scope+11, it reads"),
        ["sql.scope+11"]
    );
    assert_eq!(
        pins("> `[spec:pgorm:req:sql.ddl.view+1/test]`"),
        ["sql.ddl.view+1"]
    );

    assert!(pins("> (`[spec:pgorm:req:sql.ddl.view]`), `sql.ast.copy`").is_empty());
    assert!(pins("> `1e300` as `1e+300`, always at `+00:00`").is_empty());
    assert!(pins("> `+1 months -2 days +3000004 microseconds`").is_empty());
    assert!(pins("> `Value.x+1`, `a_b.c+2`, `N+1`, `x + 1`").is_empty());
}
