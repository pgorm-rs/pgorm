mod fixture;
pub mod process;
pub mod result;

use fixture::Fixture;
use result::{ScanResult, aggregate, detected, interpret, inventory, verdict};
use serde::Serialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, BTreeSet}, fs::{self, File}, path::{Path, PathBuf}, time::{SystemTime, UNIX_EPOCH}};
use tokio::time::Instant;

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

pub fn read_json(path: &Path) -> Result<Value> { Ok(serde_json::from_slice(&fs::read(path)?)?) }

pub fn write_json(path: &Path, data: &impl Serialize) -> Result<()> {
    let temporary = path.with_extension("json.tmp");
    fs::write(&temporary, format!("{}\n", serde_json::to_string_pretty(data)?))?;
    fs::rename(temporary, path)?;
    Ok(())
}

fn hash(bytes: impl AsRef<[u8]>) -> String { Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect() }
pub fn digest(path: &Path) -> Result<String> { Ok(hash(fs::read(path)?)) }

#[derive(Debug)]
pub struct Options {
    pub profile: String,
    pub subset: Vec<String>,
    pub artifacts: PathBuf,
    pub baseline_only: bool,
    pub direct_regressions: bool,
    pub python: String,
}

impl Options {
    pub fn parse(args: impl Iterator<Item = String>, root: &Path) -> Result<Self> {
        let mut options = Self {
            profile: "smoke".into(), subset: Vec::new(),
            artifacts: root.join("target/sqlmap").join(format!("rust-{}-{}", SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs(), std::process::id())),
            baseline_only: false, direct_regressions: false, python: "python3".into(),
        };
        let mut args = args.peekable();
        while let Some(arg) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("missing value for {arg}"));
            match arg.as_str() {
                "--profile" => options.profile = value()?,
                "--case" => options.subset.push(value()?),
                "--artifacts" => options.artifacts = value()?.into(),
                "--python" => options.python = value()?,
                "--baseline-only" => options.baseline_only = true,
                "--direct-regressions" => options.direct_regressions = true,
                _ => return Err(format!("unknown option: {arg}").into()),
            }
        }
        if !["smoke", "full"].contains(&options.profile.as_str()) { return Err("profile must be smoke or full".into()); }
        if !options.artifacts.is_absolute() { options.artifacts = std::env::current_dir()?.join(options.artifacts); }
        Ok(options)
    }
}

// [spec:pgorm:req:security.sqlmap.artifacts]
async fn source_identity(root: &Path) -> Result<Value> {
    let mut command = process::command("git", &["ls-files", "-co", "--exclude-standard", "-z"]);
    command.current_dir(root);
    let paths = process::output(&mut command, None, 120).await?;
    let mut files = BTreeMap::new();
    for path in paths.split('\0').filter(|p| !p.is_empty()) {
        let file = root.join(path);
        if file.is_file() && !path.starts_with(".nplan/") && !path.starts_with("plan/") {
            files.insert(path.to_owned(), digest(&file)?);
        }
    }
    let mut command = process::command("git", &["rev-parse", "HEAD"]);
    command.current_dir(root);
    let head = process::output(&mut command, None, 120).await?;
    let digest = hash(serde_json::to_vec(&files)?);
    Ok(json!({"head":head,"files":files,"digest":digest}))
}

// [spec:pgorm:req:security.sqlmap.pin]
async fn scanner(pins: &Value, python: &str, cache: &Path, artifacts: &Path) -> Result<PathBuf> {
    let version = process::run(python, &["--version"]).await?;
    if version != format!("Python {}", pins["python"].as_str().ok_or("missing Python pin")?) {
        return Err(format!("Python {} required; got {version}", pins["python"]).into());
    }
    fs::create_dir_all(cache)?;
    let archive = cache.join("sqlmap.tar.gz");
    if !archive.exists() {
        let temporary = artifacts.join("sqlmap.tar.gz");
        process::run("curl", &["--fail", "--silent", "--show-error", "--location", "--max-time", "60", "--output", temporary.to_str().ok_or("non-UTF8 archive path")?, pins["sqlmap"]["archive"].as_str().ok_or("missing sqlmap archive pin")?]).await?;
        if digest(&temporary)? != pins["sqlmap"]["sha256"] { return Err("sqlmap archive content does not match pins.json".into()); }
        fs::copy(&temporary, &archive)?;
        fs::remove_file(temporary)?;
    }
    if digest(&archive)? != pins["sqlmap"]["sha256"] { return Err("sqlmap archive content does not match pins.json".into()); }
    let source = artifacts.join("scanner-source");
    fs::create_dir(&source)?;
    process::run("tar", &["-xzf", archive.to_str().ok_or("non-UTF8 archive path")?, "-C", source.to_str().ok_or("non-UTF8 source path")?]).await?;
    let children = fs::read_dir(&source)?.collect::<std::io::Result<Vec<_>>>()?;
    if children.len() != 1 { return Err("unexpected upstream archive layout".into()); }
    let script = children[0].path().join("sqlmap.py");
    if !script.is_file() { return Err("missing upstream sqlmap.py".into()); }
    Ok(script)
}

pub fn target(base: &str, case: &Value, mode: &str) -> Result<String> {
    let id = case["id"].as_str().ok_or("missing case id")?;
    let mut url = url::Url::parse(&format!("{base}/case/{mode}/{id}"))?;
    url.query_pairs_mut().append_pair("input", case["baseline"].as_str().ok_or("missing baseline")?);
    Ok(url.into())
}

pub fn scanner_args(python: &str, script: &Path, target: &str, technique: &str, profile: &Value, case: &Value, output: &Path) -> Vec<String> {
    let mut args = vec![python.into(), script.to_string_lossy().into_owned(), "--url".into(), target.into()];
    args.extend(["-p", "input", "--dbms", "PostgreSQL", "--batch", "--flush-session", "--fresh-queries", "--ignore-proxy", "--disable-coloring", "--technique", technique, "--level"].map(str::to_owned));
    args.push(profile["level"].to_string());
    args.push("--risk".into()); args.push(profile["risk"].to_string());
    args.extend(["--threads", "1", "--retries", "0", "--timeout", "15", "--time-sec", "1", "--union-cols", "1-4", "--output-dir"].map(str::to_owned));
    args.push(output.join("session").to_string_lossy().into_owned());
    args.push("--report-json".into()); args.push(output.join("scanner.json").to_string_lossy().into_owned());
    args.extend(["--answers", "extending=N,include=N,fuzzy=N", "-v", "2"].map(str::to_owned));
    for option in ["prefix", "suffix"] {
        if let Some(value) = case[option].as_str() { args.push(format!("--{option}")); args.push(value.into()); }
    }
    args
}

async fn count(fixture: &Fixture, key: &str) -> Result<u64> {
    let (status, counts) = process::http(&format!("{}/counts", fixture.url)).await?;
    if status != 200 || !counts.is_object() { return Err("request accounting unavailable".into()); }
    match counts.get(key) {
        Some(value) => value.as_u64().ok_or_else(|| "malformed request counter".into()),
        None => Ok(0),
    }
}

struct Campaign<'a> {
    options: &'a Options,
    profile: Value,
    script: PathBuf,
}

impl Campaign<'_> {
    async fn scan(&self, fixture: &Fixture, case: &Value, technique: &str, mode: &str) -> Result<ScanResult> {
        let started = Instant::now();
        let id = case["id"].as_str().ok_or("missing case id")?;
        let output = self.options.artifacts.join(format!("{id}-{technique}-{mode}"));
        fs::create_dir(&output)?;
        let target = target(&fixture.url, case, mode)?;
        let args = scanner_args(&self.options.python, &self.script, &target, technique, &self.profile, case, &output);
        write_json(&output.join("command.json"), &args)?;
        // Keep the scanner invocation even when accounting is unavailable.
        let before = count(fixture, &format!("{mode}/{id}")).await;
        let invocation = async {
            let log = File::create(output.join("scanner.log"))?;
            let mut command = process::command(&args[0], &args[1..].iter().map(String::as_str).collect::<Vec<_>>());
            command.stdout(log.try_clone()?).stderr(log);
            let mut process = process::Process::spawn(&mut command)?;
            let status = process.wait(self.profile["case_timeout_seconds"].as_u64().ok_or("missing scan deadline")?).await?;
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(status.and_then(|s| {
                use std::os::unix::process::ExitStatusExt;
                s.code().or_else(|| s.signal().map(|s| -s))
            }))
        }.await;
        let after = count(fixture, &format!("{mode}/{id}")).await;
        let counts = match (before, after) {
            (Ok(before), Ok(after)) => after.checked_sub(before).ok_or("request counter moved backwards".to_owned()),
            (Err(error), _) | (_, Err(error)) => Err(format!("request accounting failed: {error}")),
        };
        let report = fixture::scanner_report(&output);
        let log = fs::read_to_string(output.join("scanner.log")).unwrap_or_default();
        let mut result = interpret(&report, &log, invocation.as_ref().ok().copied().flatten(), &target, counts.as_ref().copied().unwrap_or(0));
        if let Err(error) = invocation { result.complete = false; result.reason.push_str(&format!("; scanner invocation failed: {error}")); }
        if let Err(error) = counts { result.complete = false; result.reason.push_str(&format!("; {error}")); }
        result.seconds = Some((started.elapsed().as_secs_f64() * 1000.0).round() / 1000.0);
        write_json(&output.join("result.json"), &result)?;
        Ok(result)
    }

    async fn retained_scan(&self, fixture: &Fixture, case: &Value, technique: &str, mode: &str) -> ScanResult {
        match self.scan(fixture, case, technique, mode).await {
            Ok(result) => result,
            Err(error) => {
                let id = case["id"].as_str().unwrap_or("unknown");
                let output = self.options.artifacts.join(format!("{id}-{technique}-{mode}"));
                let report = fixture::scanner_report(&output);
                let mut result = interpret(&report, "", None, &target(&fixture.url, case, mode).unwrap_or_default(), 0);
                result.reason = format!("scan infrastructure failure: {error}");
                let _ = write_json(&output.join("result.json"), &result);
                result
            }
        }
    }

    async fn pair(&self, fixture: &Fixture, case: &Value, technique: &str) -> Result<Value> {
        let id = case["id"].as_str().ok_or("missing case id")?;
        let mut baselines = BTreeMap::new();
        for mode in ["control", "protected"] {
            let baseline = match process::http(&target(&fixture.url, case, mode)?).await {
                Ok((status, body)) => json!({"status":status, "body":body}),
                Err(error) => json!({"status":null,"body":{},"error":error.to_string()}),
            };
            baselines.insert(mode, baseline);
        }
        let baseline_ok = baselines.values().all(|b| b["status"] == 200 && b["body"]["invariant"] == true);
        if self.options.baseline_only {
            return Ok(json!({"outcome":"incomplete","reason":if baseline_ok {"baseline diagnostic"} else {"failed baseline"}, "baselines":baselines}));
        }
        let control = self.retained_scan(fixture, case, technique, "control").await;
        let mut protected = self.retained_scan(fixture, case, technique, "protected").await;
        let invariant = match fixture::request_invariant(&self.options.artifacts, id) {
            Ok(invariant) => invariant,
            Err(error) => { protected.complete = false; protected.reason.push_str(&format!("; request evidence unreadable: {error}")); true }
        };
        let outcome = verdict(&control, &protected, technique, baseline_ok, invariant);
        let reason = match outcome {
            "vulnerable" => "protected finding or invariant violation".into(),
            "invalid-control" => format!("control did not detect GET input with technique {technique}"),
            "incomplete" if !baseline_ok => "failed baseline".into(),
            "incomplete" => format!("control: {}; protected: {}", control.reason, protected.reason),
            _ => "completed with detected control and clean protected route".into(),
        };
        Ok(json!({"outcome":outcome,"reason":reason,"api":case["api"],"control_detected":detected(&control,technique),"control":control,"protected":protected,"baselines":baselines,"invariant":invariant}))
    }
}

fn summarize(report: &Value) -> String {
    let mut outcomes = BTreeMap::<String, usize>::new();
    let mut completed = 0;
    let mut attempted = 0;
    let mut findings = Vec::new();
    let mut undetected = Vec::new();
    let mut infrastructure = Vec::new();
    if let Some(results) = report["results"].as_object() {
        for (key, result) in results {
            *outcomes.entry(result["outcome"].as_str().unwrap_or("incomplete").into()).or_default() += 1;
            for mode in ["control", "protected"] {
                if result[mode].is_object() {
                    attempted += 1;
                    if result[mode]["complete"] == true { completed += 1; }
                    else { infrastructure.push(format!("{key}-{mode}: {}", result[mode]["reason"].as_str().unwrap_or("missing result"))); }
                }
            }
            if result["protected"]["findings"].as_array().is_some_and(|f| !f.is_empty()) || result["invariant"] == false { findings.push(key.clone()); }
            if result["control_detected"] == false { undetected.push(key.clone()); }
        }
    }
    format!("{}{}: {}\nScans: {completed} complete, {attempted} attempted\nOutcomes: {outcomes:?}\nProtected findings: {}\nUndetected controls: {}\nInfrastructure failures: {}\n{}\nCleanup: {}\nRun error: {}\nDirect regressions: {}\n",
        report["profile"].as_str().unwrap_or("unknown"), if report["subset"].as_array().is_some_and(|s| !s.is_empty()) {" subset"} else {""},
        if report["pass"] == true {"PASS"} else {"FAIL"}, findings.join(", "), undetected.join(", "), infrastructure.len(), infrastructure.join("\n"), report["cleanup_errors"], report["error"], report["direct_regressions"])
}

async fn execute(options: &Options, root: &Path, here: &Path, report: &mut Value, fixture: &mut Fixture) -> Result<()> {
    let pins = read_json(&here.join("pins.json"))?;
    let manifest = read_json(&here.join("cases.json"))?;
    let profile = read_json(&here.join("profiles.json"))?[&options.profile].clone();
    let mut work = inventory(&manifest, &profile, &options.subset)?;
    if options.baseline_only {
        let mut seen = BTreeSet::new();
        work.retain(|(c, _)| seen.insert(c["id"].as_str().unwrap_or_default().to_owned()));
    }
    let expected: Vec<_> = work.iter().map(|(c,t)| format!("{}-{t}", c["id"].as_str().unwrap_or_default())).collect();
    report["expected"] = json!(expected);
    report["expected_scans"] = json!(expected.iter().flat_map(|key| [format!("{key}-control"),format!("{key}-protected")]).collect::<Vec<_>>());
    report["results"] = json!(expected.iter().map(|key| (key.clone(), json!({"outcome":"incomplete","reason":"not started"}))).collect::<BTreeMap<_,_>>());
    report["pins"] = pins.clone();
    report["source"] = source_identity(root).await?;
    report["manifest_sha256"] = digest(&here.join("cases.json"))?.into();
    report["profile_sha256"] = digest(&here.join("profiles.json"))?.into();
    report["effective_profile"] = profile.clone();
    write_json(&options.artifacts.join("report.json"), report)?;
    // Keep the manifests beside results so changing a checkout cannot change their meaning.
    for file in ["cases.json", "profiles.json", "pins.json"] { fs::copy(here.join(file), options.artifacts.join(file))?; }
    let script = scanner(&pins, &options.python, &root.join("target/sqlmap-cache"), &options.artifacts).await?;
    let manifest_path = here.join("adapter/Cargo.toml");
    process::run("cargo", &["build", "--locked", "--manifest-path", manifest_path.to_str().ok_or("non-UTF8 manifest path")?, "--bin", "pgorm-sqlmap-adapter"]).await?;
    let adapter = std::env::current_exe()?.with_file_name("pgorm-sqlmap-adapter");
    report["binaries"] = json!({"adapter_sha256":digest(&adapter)?,"harness_sha256":digest(&std::env::current_exe()?)?});
    fixture.start(&pins, &adapter).await?;
    report["database"] = fixture.settings.clone();
    let campaign = Campaign { options, profile, script };
    for (case, technique) in work {
        let key = format!("{}-{technique}", case["id"].as_str().ok_or("missing case id")?);
        println!("{key}: baseline");
        let result = match campaign.pair(fixture, &case, &technique).await {
            Ok(result) => result,
            Err(error) => json!({"outcome":"incomplete","reason":format!("case infrastructure failure: {error}")}),
        };
        println!("{key}: {}", result["outcome"].as_str().unwrap_or("incomplete"));
        report["results"][&key] = result;
        write_json(&options.artifacts.join("report.json"), report)?;
    }
    if options.direct_regressions {
        println!("Running direct SQL security regressions");
        report["direct_regressions"] = match fixture.direct_regressions(root).await {
            Ok(result) => result,
            Err(error) => json!({"pass":false,"error":error.to_string()}),
        };
    }
    let final_source = source_identity(root).await?;
    report["source_unchanged"] = json!(report["source"] == final_source);
    report["source_after"] = final_source;
    if report["source_unchanged"] != true { return Err("source changed during campaign".into()); }
    Ok(())
}

pub async fn run(options: Options) -> Result<bool> {
    let here = Path::new(env!("CARGO_MANIFEST_DIR")).parent().ok_or("missing harness directory")?;
    let root = here.parent().and_then(Path::parent).ok_or("missing repository root")?;
    if let Some(parent) = options.artifacts.parent() { fs::create_dir_all(parent)?; }
    fs::create_dir(&options.artifacts)?;
    let mut report = json!({"profile":options.profile,"subset":options.subset,"results":{},"cleanup_errors":[],"pass":false});
    write_json(&options.artifacts.join("report.json"), &report)?;
    let mut fixture = Fixture::new(&options.artifacts)?;
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    let operation = tokio::select! {
        result = execute(&options, root, here, &mut report, &mut fixture) => result,
        _ = tokio::signal::ctrl_c() => Err("cancelled by SIGINT".into()),
        _ = term.recv() => Err("cancelled by SIGTERM".into()),
    };
    if let Err(error) = operation { report["error"] = error.to_string().into(); eprintln!("{error}"); }
    let cleanup = fixture.close().await;
    let expected: Vec<String> = serde_json::from_value(report["expected"].clone()).unwrap_or_default();
    let results: BTreeMap<String, Value> = serde_json::from_value(report["results"].clone()).unwrap_or_default();
    report["cleanup_errors"] = json!(cleanup);
    report["pass"] = json!(!options.baseline_only && report.get("error").is_none() && aggregate(&expected, &results, &cleanup)
        && (!options.direct_regressions || report["direct_regressions"]["pass"] == true));
    write_json(&options.artifacts.join("report.json"), &report)?;
    let summary = summarize(&report);
    fs::write(options.artifacts.join("summary.txt"), &summary)?;
    println!("{summary}Artifacts: {}", options.artifacts.display());
    Ok(report["pass"] == true)
}
