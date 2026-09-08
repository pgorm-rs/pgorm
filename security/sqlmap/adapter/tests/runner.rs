use pgorm_sqlmap_adapter::harness::{self, Options, process, result::{self, ScanResult}};
use serde_json::{Value, json};
use std::{collections::BTreeMap, path::{Path, PathBuf}};

const TARGET: &str = "http://127.0.0.1:56392/case/protected/select?input=alice";
const LOG: &str = "GET parameter 'input' does not seem to be injectable\n[*] ending @ 12:00:00";

fn clean_report() -> Value { json!({"success":true,"meta":{"url":TARGET.split('?').next()},"data":[],"error":[result::CLEAN_MESSAGE]}) }
fn clean() -> ScanResult { result::interpret(&clean_report(), LOG, Some(0), TARGET, 25) }
fn finding(technique: &str, parameter: &str, place: &str) -> Value {
    json!({"parameter":parameter,"place":place,"data":[{"technique":technique,"payload":"input=alice' AND 1=1 --"}]})
}
fn detection() -> ScanResult {
    let mut report = clean_report();
    report["data"] = json!([{"type_name":"TECHNIQUES","value":[finding("boolean-based blind", "input", "GET")]}]);
    result::interpret(&report, LOG, Some(0), TARGET, 25)
}

// [spec:pgorm:req:security.sqlmap.runner-tests]
// [spec:pgorm:req:security.sqlmap.runner-tests/test]
// [spec:pgorm:req:security.sqlmap.execution/test]
#[test]
fn actual_sqlmap_urls_omit_query_but_match_every_endpoint_component() {
    for (raw, log) in [(include_str!("fixtures/control.json"), include_str!("fixtures/control.log")), (include_str!("fixtures/protected.json"), include_str!("fixtures/protected.log"))] {
        let report: Value = serde_json::from_str(raw).unwrap();
        let target = format!("{}?input=alice", report["meta"]["url"].as_str().unwrap());
        let complete = result::interpret(&report, log, Some(0), &target, 25);
        assert!(complete.complete, "{}", complete.reason);
        for (field, stale) in [("scheme", target.replace("http:","https:")), ("hostname", target.replace("127.0.0.1","localhost")), ("port", target.replace(":56392",":56393")), ("path", target.replace("/select","/delete"))] {
            let failed = result::interpret(&report, log, Some(0), &stale, 25);
            assert!(!failed.complete);
            assert!(failed.reason.contains(field), "{}", failed.reason);
            assert_eq!(failed.findings, complete.findings);
        }
        let inactive = result::interpret(&report, log, Some(0), &target, 0);
        assert!(!inactive.complete);
        assert!(inactive.reason.contains("inactive route"));
        assert_eq!(inactive.findings, complete.findings);
        let crashed = result::interpret(&report, log, Some(1), &target, 25);
        assert!(crashed.reason.contains("exited 1"));
        assert_eq!(crashed.findings, complete.findings);
    }
}

#[test]
fn zero_exit_without_valid_output_is_incomplete() {
    for report in [Value::Null, json!({}), json!([]), json!({"success":false}), json!({"success":true}), json!({"success":true,"data":{},"error":[]})] {
        assert!(!result::interpret(&report, LOG, Some(0), TARGET, 25).complete);
    }
}

#[test]
fn malformed_findings_and_wrong_injection_points_are_retained() {
    for finding in [Value::Null, json!("bad"), finding("boolean-based blind", "other", "GET"), finding("boolean-based blind", "input", "Cookie"), finding("unknown", "input", "GET")] {
        let mut report = clean_report();
        report["data"] = json!([{"type_name":"TECHNIQUES","value":[finding.clone()]}]);
        let result = result::interpret(&report, LOG, Some(0), TARGET, 25);
        assert!(!result.complete);
        assert!(result.reason.contains("injection parameter"));
        assert_eq!(result.findings, vec![finding]);
    }
}

#[test]
fn malformed_metadata_never_panics_or_completes() {
    for meta in [Value::Null, json!([]), json!(5), json!({"url":[]}), json!({"url":"http://127.0.0.1:invalid/"}), json!({"url":"not a url"})] {
        let mut report = clean_report(); report["meta"] = meta;
        let result = result::interpret(&report, LOG, Some(0), TARGET, 25);
        assert!(!result.complete); assert!(result.reason.contains("URL"));
    }
}

#[test]
fn crashes_cancel_and_timeouts_are_incomplete() {
    for code in [Some(1), Some(-9), Some(-15), None] {
        assert!(!result::interpret(&clean_report(), LOG, code, TARGET, 25).complete);
    }
}

#[test]
fn missing_terminal_evidence_is_incomplete() {
    for log in ["", "[*] ending @", "parameter 'input' does not seem to be injectable"] {
        assert!(!result::interpret(&clean_report(), log, Some(0), TARGET, 25).complete);
    }
}

#[test]
fn transport_failures_and_skipped_parameters_are_incomplete() {
    for message in ["connection timed out", "unable to connect", "connection reset", "user aborted", "skipping parameter"] {
        let result = result::interpret(&clean_report(), &format!("{LOG}{message}"), Some(0), TARGET, 25);
        assert!(!result.complete); assert!(result.reason.contains("transport"));
    }
}

#[test]
fn scanner_errors_are_not_clean_negatives() {
    let mut report = clean_report(); report["error"] = json!(["unexpected internal exception"]);
    let result = result::interpret(&report, LOG, Some(0), TARGET, 25);
    assert!(!result.complete); assert_eq!(result.errors, vec![json!("unexpected internal exception")]);
}

// [spec:pgorm:req:security.sqlmap.outcomes/test]
// [spec:pgorm:req:security.sqlmap.controls/test]
#[test]
fn verdict_requires_detected_control_complete_protected_and_baselines() {
    assert_eq!(result::verdict(&detection(), &clean(), "B", true, true), "pass");
    assert_eq!(result::verdict(&clean(), &clean(), "B", true, true), "invalid-control");
    assert_eq!(result::verdict(&detection(), &clean(), "E", true, true), "invalid-control");
    let mut incomplete = clean(); incomplete.complete = false;
    assert_eq!(result::verdict(&detection(), &clean(), "B", false, true), "incomplete");
    assert_eq!(result::verdict(&incomplete, &clean(), "B", true, true), "incomplete");
    assert_eq!(result::verdict(&detection(), &incomplete, "B", true, true), "incomplete");
}

#[test]
fn protected_findings_and_invariant_violations_always_fail() {
    let mut incomplete = clean(); incomplete.complete = false;
    for control in [clean(), detection(), incomplete.clone()] {
        assert_eq!(result::verdict(&control, &detection(), "B", false, true), "vulnerable");
        assert_eq!(result::verdict(&control, &incomplete, "B", true, false), "vulnerable");
    }
}

// [spec:pgorm:req:security.sqlmap.verdict/test]
#[test]
fn empty_missing_extra_duplicate_skipped_and_failed_cleanup_cannot_pass() {
    let pass = BTreeMap::from([("a".into(), json!({"outcome":"pass"}))]);
    assert!(result::aggregate(&["a".into()], &pass, &[]));
    assert!(!result::aggregate(&[], &BTreeMap::new(), &[]));
    assert!(!result::aggregate(&["a".into()], &BTreeMap::new(), &[]));
    assert!(!result::aggregate(&["a".into(), "b".into()], &pass, &[]));
    assert!(!result::aggregate(&["a".into(), "a".into()], &pass, &[]));
    assert!(!result::aggregate(&[], &pass, &[]));
    assert!(!result::aggregate(&["a".into()], &pass, &["container removal failed".into()]));
    for outcome in ["incomplete", "invalid-control", "vulnerable", "skipped"] {
        assert!(!result::aggregate(&["a".into()], &BTreeMap::from([("a".into(),json!({"outcome":outcome}))]), &[]));
    }
}

// [spec:pgorm:req:security.sqlmap.profiles/test]
// [spec:pgorm:req:security.sqlmap.matrix/test]
#[test]
fn profiles_keep_all_420_full_and_six_smoke_invocations() {
    let manifest: Value = serde_json::from_str(include_str!("../../cases.json")).unwrap();
    let profiles: Value = serde_json::from_str(include_str!("../../profiles.json")).unwrap();
    assert_eq!(manifest["cases"].as_array().unwrap().len(), 35);
    assert_eq!(result::inventory(&manifest, &profiles["full"], &[]).unwrap().len() * 2, 420);
    assert_eq!(result::inventory(&manifest, &profiles["smoke"], &[]).unwrap().len() * 2, 6);
    assert!(result::inventory(&manifest, &profiles["smoke"], &["insert".into()]).is_err());
    assert!(result::inventory(&manifest, &profiles["full"], &["select".into(),"select".into()]).is_err());
    let mut profile = profiles["full"].clone(); profile["techniques"] = json!([]);
    assert!(result::inventory(&manifest, &profile, &[]).is_err());
    profile["cases"] = json!(["unknown"]);
    assert!(result::inventory(&manifest, &profile, &[]).is_err());
}

#[test]
fn command_options_and_query_encoding_match_the_python_runner() {
    let manifest: Value = serde_json::from_str(include_str!("../../cases.json")).unwrap();
    let profiles: Value = serde_json::from_str(include_str!("../../profiles.json")).unwrap();
    let mut case = manifest["cases"][0].clone(); case["baseline"] = json!("a b'+&%");
    assert_eq!(harness::target("http://127.0.0.1:12", &case, "control").unwrap(), "http://127.0.0.1:12/case/control/select?input=a+b%27%2B%26%25");
    let args = harness::scanner_args("python3", Path::new("scanner.py"), TARGET, "B", &profiles["full"], &case, Path::new("output"));
    assert_eq!(args, ["python3", "scanner.py", "--url", TARGET, "-p", "input", "--dbms", "PostgreSQL", "--batch", "--flush-session", "--fresh-queries", "--ignore-proxy", "--disable-coloring", "--technique", "B", "--level", "3", "--risk", "2", "--threads", "1", "--retries", "0", "--timeout", "15", "--time-sec", "1", "--union-cols", "1-4", "--output-dir", "output/session", "--report-json", "output/scanner.json", "--answers", "extending=N,include=N,fuzzy=N", "-v", "2"]);
}

#[tokio::test]
async fn process_timeout_and_cancellation_stop_children() {
    let mut child = process::Process::spawn(&mut process::command("sh", &["-c", "sleep 120 & wait"])).unwrap();
    assert!(child.wait(1).await.unwrap().is_none());
    assert!(child.child.try_wait().unwrap().is_some());
    let mut child = process::Process::spawn(&mut process::command("sh", &["-c", "sleep 120 & wait"])).unwrap();
    child.stop().await.unwrap();
    assert!(child.child.try_wait().unwrap().is_some());
}

// [spec:pgorm:req:security.sqlmap.artifacts/test]
#[test]
fn setup_failure_retains_inventory_report_summary_and_nonzero_status() {
    let artifacts = std::env::temp_dir().join(format!("sqlmap-runner-test-{}", std::process::id()));
    let status = std::process::Command::new(env!("CARGO_BIN_EXE_sqlmap-harness"))
        .args(["--profile", "full", "--python", "/nonexistent/sqlmap-python", "--artifacts"])
        .arg(&artifacts).output().unwrap();
    assert!(!status.status.success());
    let report = harness::read_json(&artifacts.join("report.json")).unwrap();
    assert_eq!(report["expected_scans"].as_array().unwrap().len(), 420);
    assert_eq!(report["results"].as_object().unwrap().len(), 210);
    assert_eq!(report["pass"], false);
    assert!(report["error"].is_string());
    assert!(artifacts.join("summary.txt").is_file());
    std::fs::remove_dir_all(artifacts).unwrap();
}

#[test]
fn cli_preserves_profile_subset_and_diagnostic_options() {
    let options = Options::parse(["--profile", "full", "--case", "select", "--baseline-only", "--direct-regressions"].into_iter().map(str::to_owned), Path::new("/tmp")).unwrap();
    assert_eq!(options.profile, "full"); assert_eq!(options.subset, vec!["select"]);
    assert!(options.baseline_only && options.direct_regressions);
    assert!(Options::parse(["--profile".into()].into_iter(), Path::new("/tmp")).is_err());
}

/// Optional corpus check: reinterprets every retained Python scan, including failures.
#[test]
fn parity_against_python_campaign() {
    let Some(artifact_dir) = std::env::var_os("SQLMAP_PARITY_REPORT") else { return; };
    let artifact_dir = PathBuf::from(artifact_dir);
    let report = harness::read_json(&artifact_dir.join("report.json")).unwrap();
    let mut checked = 0;
    for key in report["expected"].as_array().unwrap() {
        for mode in ["control", "protected"] {
            let output = artifact_dir.join(format!("{}-{mode}", key.as_str().unwrap()));
            let expected = harness::read_json(&output.join("result.json")).unwrap();
            let scanner = harness::read_json(&output.join("scanner.json")).unwrap_or(Value::Null);
            let log = std::fs::read_to_string(output.join("scanner.log")).unwrap();
            let args = harness::read_json(&output.join("command.json")).unwrap();
            let args = args.as_array().unwrap();
            let target = args[args.iter().position(|a| a == "--url").unwrap() + 1].as_str().unwrap();
            let actual = result::interpret(&scanner, &log, expected["returncode"].as_i64().map(|c| c as i32), target, expected["requests"].as_u64().unwrap());
            assert_eq!(actual.complete, expected["complete"].as_bool().unwrap(), "{}", output.display());
            assert_eq!(json!(actual.findings), expected["findings"], "{}", output.display());
            assert_eq!(json!(actual.errors), expected["errors"], "{}", output.display());
            assert_eq!(actual.reason, expected["reason"].as_str().unwrap(), "{}", output.display());
            checked += 1;
        }
    }
    assert_eq!(checked, 420);
}
