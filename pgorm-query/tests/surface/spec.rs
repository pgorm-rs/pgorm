//! The spec's prose, read for the names and rule ids it writes in backticks.
//!
//! A rule is a blockquote opened by its `> [spec:pgorm:<verb>:<id>]` marker.
//! Its lines are joined with single spaces, paragraphs kept apart, so a code
//! span the text wraps across a line still reads as one span and nothing here
//! depends on where a line breaks.

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub fn spec_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../docs/spec")
}

/// One rule's text, a string per paragraph.
pub struct Rule {
    pub id: String,
    pub paragraphs: Vec<String>,
}

impl Rule {
    /// Read the rule `id` out of `docs/spec/<file>`.
    pub fn read(file: &str, id: &str) -> Self {
        let file = spec_dir().join(file);
        let text = fs::read_to_string(&file)
            .unwrap_or_else(|error| panic!("reading {}: {error}", file.display()));
        let mut lines = text.lines();
        lines
            .by_ref()
            .find(|line| defined_id(line) == Some(id))
            .unwrap_or_else(|| panic!("{} defines no rule `{id}`", file.display()));
        let mut paragraphs = vec![String::new()];
        for line in lines {
            let Some(body) = line.strip_prefix('>') else {
                break;
            };
            let body = body.trim();
            let paragraph = paragraphs.last_mut().expect("never empty");
            if body.is_empty() {
                if !paragraph.is_empty() {
                    paragraphs.push(String::new());
                }
            } else {
                if !paragraph.is_empty() {
                    paragraph.push(' ');
                }
                paragraph.push_str(body);
            }
        }
        paragraphs.retain(|paragraph| !paragraph.is_empty());
        Self {
            id: id.to_owned(),
            paragraphs,
        }
    }

    pub fn text(&self) -> String {
        self.paragraphs.join("\n\n")
    }

    /// The paragraph containing `needle`, which must be exactly one: a
    /// sentinel phrase that came to appear twice would otherwise pick a
    /// paragraph silently.
    pub fn paragraph_with(&self, needle: &str) -> &str {
        let mut found = self
            .paragraphs
            .iter()
            .filter(|paragraph| paragraph.contains(needle));
        match (found.next(), found.next()) {
            (Some(paragraph), None) => paragraph,
            (None, _) => panic!("no paragraph of `{}` contains {needle:?}", self.id),
            (Some(_), Some(_)) => panic!(
                "more than one paragraph of `{}` contains {needle:?}",
                self.id
            ),
        }
    }
}

/// The id a line defines when it is a rule marker, `> [spec:pgorm:req:a.b+3]`.
fn defined_id(line: &str) -> Option<&str> {
    let marker = line.strip_prefix('>')?.trim();
    let rest = marker.strip_prefix("[spec:pgorm:")?.strip_suffix(']')?;
    let (_verb, id) = rest.split_once(':')?;
    Some(id.split_once('+').map_or(id, |(id, _version)| id))
}

/// Every rule id the spec defines, across every file under `docs/spec`.
pub fn defined_rule_ids() -> BTreeSet<String> {
    fn walk(dir: &Path, ids: &mut BTreeSet<String>) {
        let entries =
            fs::read_dir(dir).unwrap_or_else(|error| panic!("reading {}: {error}", dir.display()));
        for entry in entries {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                walk(&path, ids);
            } else if path.extension().is_some_and(|extension| extension == "md") {
                let text = fs::read_to_string(&path)
                    .unwrap_or_else(|error| panic!("reading {}: {error}", path.display()));
                ids.extend(text.lines().filter_map(defined_id).map(str::to_owned));
            }
        }
    }
    let mut ids = BTreeSet::new();
    walk(&spec_dir(), &mut ids);
    ids
}

/// The text of each inline code span, in order.
pub fn code_spans(text: &str) -> impl Iterator<Item = &str> {
    text.split('`').skip(1).step_by(2)
}

/// A Rust identifier, as an item name is written.
pub fn is_identifier(span: &str) -> bool {
    let mut chars = span.chars();
    chars
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Spelled the way a type or trait is, `UpperCamelCase`: what a code span in
/// the register must mean when it is not an export. All-capitals spans are
/// SQL keywords there (`CASE`, `JSON_TABLE`), and lowercase ones are module
/// or function names, so neither is presumed to be an item.
pub fn is_item_shaped(span: &str) -> bool {
    is_identifier(span)
        && span.starts_with(|c: char| c.is_ascii_uppercase())
        && span.contains(|c: char| c.is_ascii_lowercase())
}

/// A rule id as the prose cites one, `sql.ddl.create-table`: two or more
/// dot-separated lowercase parts.
pub fn is_rule_id(span: &str) -> bool {
    let mut parts = span.split('.');
    let first_is_word = parts.next().is_some_and(|first| {
        first.starts_with(|c: char| c.is_ascii_lowercase()) && is_rule_part(first)
    });
    let mut rest = parts.peekable();
    first_is_word && rest.peek().is_some() && rest.all(is_rule_part)
}

fn is_rule_part(part: &str) -> bool {
    !part.is_empty()
        && part
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

/// What the register of `sql.surface` records: each added item's name, with
/// the rule ids the prose cites for it.
pub struct Register {
    pub entries: BTreeMap<String, Vec<String>>,
    /// Names after the last rule id: recorded, but with no rule.
    pub without_rule: Vec<String>,
}

impl Register {
    /// Read the register out of `text`, the span of the rule that holds it.
    ///
    /// An entry is the prose's own shape, one or more names and then the
    /// rule in parentheses — `Deferrability`, when a constraint's check runs
    /// (`sql.ddl.deferrability`) — so each name is attributed to the rule ids
    /// that next follow it. A code span is a name when it is an identifier
    /// that `is_export` accepts or that [`is_item_shaped`]; anything else is
    /// the prose's own vocabulary and is passed over.
    pub fn parse(text: &str, is_export: impl Fn(&str) -> bool) -> Self {
        let mut entries: BTreeMap<String, Vec<String>> = BTreeMap::new();
        let mut pending: Vec<&str> = Vec::new();
        let mut cited: Vec<&str> = Vec::new();
        let mut attribute = |pending: &mut Vec<&str>, cited: &mut Vec<&str>| {
            for name in pending.drain(..) {
                let rules = entries.entry(name.to_owned()).or_default();
                rules.extend(cited.iter().map(|&rule| rule.to_owned()));
            }
            cited.clear();
        };
        for span in code_spans(text) {
            if is_rule_id(span) {
                cited.push(span);
                continue;
            }
            if !cited.is_empty() {
                attribute(&mut pending, &mut cited);
            }
            if is_identifier(span) && (is_export(span) || is_item_shaped(span)) {
                pending.push(span);
            }
        }
        if !cited.is_empty() {
            attribute(&mut pending, &mut cited);
        }
        Self {
            entries,
            without_rule: pending.into_iter().map(str::to_owned).collect(),
        }
    }

    pub fn cited_rules(&self) -> BTreeSet<&str> {
        self.entries
            .values()
            .flatten()
            .map(String::as_str)
            .collect()
    }
}
