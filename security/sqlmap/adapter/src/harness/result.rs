use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use url::Url;

pub const CLEAN_MESSAGE: &str = "all tested parameters do not appear to be injectable.";
pub const TECHNIQUES: [(&str, &str); 6] = [
    ("B", "boolean-based blind"),
    ("E", "error-based"),
    ("U", "UNION query"),
    ("S", "stacked queries"),
    ("T", "time-based blind"),
    ("Q", "inline query"),
];

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub complete: bool,
    pub reason: String,
    pub findings: Vec<Value>,
    pub errors: Vec<Value>,
    pub requests: u64,
    pub returncode: Option<i32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seconds: Option<f64>,
}

fn valid_finding(finding: &Value) -> bool {
    finding["parameter"] == "input"
        && finding["place"] == "GET"
        && finding["data"].as_array().is_some_and(|data| {
            !data.is_empty()
                && data.iter().all(|d| {
                    TECHNIQUES.iter().any(|(_, name)| d["technique"] == *name)
                })
        })
}

// [spec:pgorm:req:security.sqlmap.execution]
pub fn interpret(
    report: &Value,
    log: &str,
    returncode: Option<i32>,
    target: &str,
    requests: u64,
) -> ScanResult {
    let mut findings = Vec::new();
    let mut failures = Vec::new();
    let mut malformed = !report["data"].is_array() || !report["error"].is_array();
    if let Some(data) = report["data"].as_array() {
        for entry in data {
            if !entry.is_object() {
                malformed = true;
            } else if entry["type_name"] == "TECHNIQUES" {
                if let Some(values) = entry["value"].as_array() {
                    findings.extend(values.iter().cloned());
                } else {
                    malformed = true;
                }
            }
        }
    }
    let errors: Vec<_> = report["error"].as_array().into_iter().flatten()
        .filter(|e| !e.as_str().is_some_and(|s| s.starts_with(CLEAN_MESSAGE)))
        .cloned().collect();
    if returncode != Some(0) {
        failures.push(match returncode {
            Some(code) => format!("scanner exited {code}"),
            None => "scanner timed out or cancelled".into(),
        });
    }
    if report["success"] != true {
        failures.push("scanner result missing or unsuccessful".into());
    }
    if malformed {
        failures.push("malformed scanner data".into());
    }
    // sqlmap's meta.url omits the query; the injection point is checked separately.
    match (report["meta"]["url"].as_str().and_then(|s| Url::parse(s).ok()), Url::parse(target)) {
        (Some(actual), Ok(wanted)) => {
            for (field, matches) in [
                ("scheme", actual.scheme() == wanted.scheme()),
                ("hostname", actual.host_str() == wanted.host_str()),
                ("port", actual.port_or_known_default() == wanted.port_or_known_default()),
                ("path", actual.path() == wanted.path()),
            ] {
                if !matches { failures.push(format!("target {field} mismatch")); }
            }
        }
        _ => failures.push("malformed scanner URL".into()),
    }
    if requests == 0 {
        failures.push("inactive route: no accounted requests".into());
    }
    let valid = findings.iter().filter(|f| valid_finding(f)).count();
    if valid != findings.len() {
        failures.push("unexpected injection parameter or malformed finding".into());
    }
    if !errors.is_empty() { failures.push("scanner reported errors".into()); }
    let lower = log.to_lowercase();
    if ["connection timed out", "unable to connect", "connection reset", "user aborted", "skipping parameter"].iter().any(|s| lower.contains(s)) {
        failures.push("transport failure or skipped parameter".into());
    }
    let tested = log.contains("parameter 'input' does not seem to be injectable") || valid > 0;
    if !tested || !log.contains("[*] ending @") {
        failures.push("missing completion evidence".into());
    }
    ScanResult {
        complete: failures.is_empty(),
        reason: if failures.is_empty() { "completed".into() } else { failures.join("; ") },
        findings, errors, requests, returncode, seconds: None,
    }
}

pub fn detected(control: &ScanResult, technique: &str) -> bool {
    TECHNIQUES.iter().find(|(id, _)| *id == technique).is_some_and(|(_, name)| {
        control.findings.iter().any(|f| valid_finding(f) && f["data"].as_array()
            .is_some_and(|data| data.iter().any(|d| d["technique"] == *name)))
    })
}

// [spec:pgorm:req:security.sqlmap.outcomes]
pub fn verdict(control: &ScanResult, protected: &ScanResult, technique: &str, baseline: bool, invariant: bool) -> &'static str {
    if !protected.findings.is_empty() || !invariant { return "vulnerable"; }
    if !baseline || !control.complete { return "incomplete"; }
    if !detected(control, technique) { return "invalid-control"; }
    if !protected.complete { return "incomplete"; }
    "pass"
}

// [spec:pgorm:req:security.sqlmap.verdict]
pub fn aggregate(expected: &[String], results: &BTreeMap<String, Value>, cleanup: &[String]) -> bool {
    let unique: BTreeSet<_> = expected.iter().collect();
    !expected.is_empty() && expected.len() == unique.len()
        && unique == results.keys().collect()
        && results.values().all(|r| r["outcome"] == "pass") && cleanup.is_empty()
}

// [spec:pgorm:req:security.sqlmap.profiles]
pub fn inventory(manifest: &Value, profile: &Value, subset: &[String]) -> Result<Vec<(Value, String)>, String> {
    let list = |v: &Value| -> Result<Vec<String>, String> {
        v.as_array().ok_or("missing inventory list")?.iter()
            .map(|v| v.as_str().map(str::to_owned).ok_or("non-string inventory entry".into())).collect()
    };
    let cases = manifest["cases"].as_array().ok_or("missing manifest cases")?;
    let mut by_id = BTreeMap::new();
    for case in cases {
        let id = case["id"].as_str().ok_or("missing case id")?;
        if by_id.insert(id.to_owned(), case).is_some() { return Err("duplicated manifest case".into()); }
    }
    let selected = list(&profile["cases"])?;
    if subset.iter().any(|s| !selected.contains(s)) { return Err("subset contains cases outside the selected profile".into()); }
    let selected = if subset.is_empty() { selected } else { subset.to_vec() };
    if selected.is_empty() || selected.len() != selected.iter().collect::<BTreeSet<_>>().len() {
        return Err("empty or duplicated case inventory".into());
    }
    let techniques = list(&profile["techniques"])?;
    if techniques.is_empty() || techniques.len() != techniques.iter().collect::<BTreeSet<_>>().len()
        || techniques.iter().any(|t| !TECHNIQUES.iter().any(|(id, _)| id == t)) {
        return Err("empty, duplicated or unknown technique inventory".into());
    }
    let mut work = Vec::new();
    for id in selected {
        let case = by_id.get(&id).ok_or(format!("unknown case {id}"))?;
        let enabled = list(&case["techniques"])?;
        if enabled.len() != enabled.iter().collect::<BTreeSet<_>>().len() { return Err("duplicated case technique".into()); }
        let before = work.len();
        for technique in enabled {
            if techniques.contains(&technique) { work.push(((*case).clone(), technique)); }
        }
        if before == work.len() { return Err(format!("no scheduled techniques for {id}")); }
    }
    Ok(work)
}
