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

use crate::spec::{citations, defined_id, read_spec_file, spec_dir, spec_files};

/// Each pinned citation in `line`, as `a.b+3`.
fn pins(line: &str) -> Vec<String> {
    citations(line)
        .filter_map(|cited| Some(format!("{}+{}", cited.id, cited.version?)))
        .collect()
}

/// A pin in prose would go stale at the cited rule's next bump. The message
/// names each one so that dropping its `+N` is the whole fix.
#[test]
fn the_spec_cites_no_rule_at_a_version() {
    let dir = spec_dir();
    let mut markers = 0;
    let mut pinned = Vec::new();
    for file in spec_files() {
        let name = file
            .strip_prefix(&dir)
            .unwrap_or(&file)
            .display()
            .to_string();
        for (index, line) in read_spec_file(&file).lines().enumerate() {
            if defined_id(line).is_some() {
                markers += 1;
                continue;
            }
            for pin in pins(line) {
                pinned.push(format!("docs/spec/{name}:{}: {pin}", index + 1));
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
    assert_eq!(
        defined_id("> [spec:pgorm:req:sql.ddl.view+1]"),
        Some("sql.ddl.view")
    );
    assert_eq!(
        defined_id("> [spec:pgorm:sem:sql.value.interval]"),
        Some("sql.value.interval")
    );
    assert_eq!(
        defined_id("> [spec:pgorm:req:query.loader.table-ref-limitation+3] rather than SQL"),
        None
    );
    assert_eq!(
        defined_id("> [spec:pgorm:req:a.b] and [spec:pgorm:req:c.d]"),
        None
    );
    assert_eq!(defined_id("> see [spec:pgorm:req:sql.ddl.view+1]"), None);
    assert_eq!(defined_id("> [spec:pgorm:req:sql.ddl.view+1/test]"), None);

    assert_eq!(
        pins("> (`[spec:pgorm:req:sql.ddl.create-table+16]`) and `exec.cursor.keyset+6`."),
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
    assert_eq!(pins("> as it ends, sql.scope+17."), ["sql.scope+17"]);
    assert_eq!(
        pins("> `[spec:pgorm:req:sql.ddl.view+1/test]`"),
        ["sql.ddl.view+1"]
    );

    assert!(pins("> (`[spec:pgorm:req:sql.ddl.view]`), `sql.ast.copy`").is_empty());
    assert!(pins("> `1e300` as `1e+300`, always at `+00:00`").is_empty());
    assert!(pins("> `+1 months -2 days +3000004 microseconds`").is_empty());
    assert!(pins("> `Value.x+1`, `a_b.c+2`, `N+1`, `x + 1`").is_empty());
}
