#!/usr/bin/env python3
"""Pinned, fixture-only sqlmap runner. Python standard library only."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import secrets
import shutil
import signal
import subprocess
import sys
import tarfile
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent.parent
TECHNIQUES = {"B": "boolean-based blind", "E": "error-based", "U": "UNION query", "S": "stacked queries", "T": "time-based blind", "Q": "inline query"}
CLEAN_MESSAGE = "all tested parameters do not appear to be injectable."


def read_json(path):
    return json.loads(Path(path).read_text())


def write_json(path, data):
    Path(path).write_text(json.dumps(data, indent=2, sort_keys=True) + "\n")


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def command(args, **kwargs):
    return subprocess.run(args, check=True, text=True, capture_output=True, timeout=120, **kwargs).stdout.strip()


# [spec:pgorm:req:security.sqlmap.pin]
def scanner(pins, cache):
    if platform.python_version() != pins["python"]:
        raise RuntimeError(f"Python {pins['python']} required; got {platform.python_version()}")
    archive = cache / "sqlmap.tar.gz"
    if not archive.exists():
        with urllib.request.urlopen(pins["sqlmap"]["archive"], timeout=60) as response:
            archive.write_bytes(response.read())
    if digest(archive) != pins["sqlmap"]["sha256"]:
        raise RuntimeError("sqlmap archive content does not match pins.json")
    # Always extract verified bytes into fresh state; never trust a mutable checkout.
    source = cache / "scanner"
    if source.exists():
        shutil.rmtree(source)
    source.mkdir()
    with tarfile.open(archive) as tar:
        tar.extractall(source, filter="data")
    children = list(source.iterdir())
    if len(children) != 1 or not (children[0] / "sqlmap.py").is_file():
        raise RuntimeError("unexpected upstream archive layout")
    return children[0] / "sqlmap.py"


def http(url):
    try:
        with urllib.request.urlopen(url, timeout=20) as response:
            return response.status, json.loads(response.read())
    except urllib.error.HTTPError as error:
        return error.code, json.loads(error.read())


def stop_process(process):
    if process is None:
        return
    # Kill the whole process group, including any scan descendants.
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        os.killpg(process.pid, signal.SIGKILL)
        process.wait(timeout=5)


# [spec:pgorm:req:security.sqlmap.isolation]
# [spec:pgorm:req:security.sqlmap.fixtures]
class Fixture:
    def __init__(self, pins, artifacts):
        self.pins = pins
        self.artifacts = artifacts
        self.name = "pgorm-sqlmap-" + secrets.token_hex(6)
        self.adapter = None
        self.container_created = False
        self.handles = []

    def start(self):
        password = secrets.token_hex(24)
        admin_password = secrets.token_hex(24)
        # Credentials travel on stdin/environment, never scanner arguments or reports.
        env = dict(os.environ, POSTGRES_PASSWORD=admin_password)
        command(["docker", "run", "--detach", "--name", self.name,
                 "--label", "pgorm.sqlmap=disposable", "--network", "bridge",
                 "--publish", "127.0.0.1::5432", "--env", "POSTGRES_PASSWORD",
                 "--mount", "type=volume,destination=/var/lib/postgresql/data", "--memory", "512m", "--cpus", "2",
                 self.pins["postgres"], "-c", "statement_timeout=10000", "-c", "lock_timeout=2000"], env=env)
        self.container_created = True
        deadline = time.monotonic() + 60
        while True:
            ready = subprocess.run(["docker", "exec", self.name, "pg_isready", "-h", "127.0.0.1", "-p", "5432", "-U", "postgres"], capture_output=True)
            if ready.returncode == 0:
                break
            if time.monotonic() > deadline:
                raise RuntimeError("PostgreSQL readiness deadline exceeded")
            time.sleep(0.25)
        self.port = command(["docker", "port", self.name, "5432/tcp"]).split(":")[-1]
        self.admin_url = f"postgresql://postgres:{admin_password}@127.0.0.1:{self.port}"
        sql = f"""CREATE ROLE fixture LOGIN PASSWORD '{password}' NOSUPERUSER NOCREATEDB NOCREATEROLE;
CREATE DATABASE harness OWNER fixture;
ALTER ROLE fixture SET search_path = fixture, pg_catalog;
ALTER ROLE fixture SET standard_conforming_strings = on;
ALTER ROLE fixture SET statement_timeout = '10s';
ALTER ROLE fixture SET lock_timeout = '2s';
"""
        command(["docker", "exec", "-i", self.name, "psql", "-U", "postgres", "-v", "ON_ERROR_STOP=1"], input=sql)
        self.settings = json.loads(command(["docker", "exec", self.name, "psql", "-U", "postgres", "-d", "harness", "-Atc",
            "SELECT json_build_object('version',version(),'encoding',current_setting('server_encoding'),'search_path','fixture, pg_catalog','standard_conforming_strings','on','statement_timeout','10s','lock_timeout','2s')"]))
        self.settings["image"] = command(["docker", "inspect", self.name, "--format", "{{.Image}}"])
        stderr = (self.artifacts / "adapter.stderr").open("w")
        self.handles.append(stderr)
        env = {k: v for k, v in os.environ.items() if k not in ("DATABASE_URL", "PGPASSWORD", "POSTGRES_PASSWORD")}
        env.update(SQLMAP_FIXTURE_URL=f"postgresql://fixture:{password}@127.0.0.1:{self.port}/harness", SQLMAP_EVIDENCE=str(self.artifacts / "requests.jsonl"))
        binary = HERE / "adapter/target/debug/pgorm-sqlmap-adapter"
        self.adapter = subprocess.Popen([str(binary)], env=env, text=True, stdout=subprocess.PIPE, stderr=stderr, start_new_session=True)
        import selectors
        selector = selectors.DefaultSelector()
        selector.register(self.adapter.stdout, selectors.EVENT_READ)
        if not selector.select(timeout=30):
            raise RuntimeError("adapter startup deadline exceeded")
        line = self.adapter.stdout.readline()
        selector.close()
        self.url = f"http://127.0.0.1:{json.loads(line)['port']}"
        return self

    def close(self):
        failures = []
        try:
            stop_process(self.adapter)
        except Exception as error:
            failures.append(str(error))
        for handle in self.handles:
            handle.close()
        if self.container_created:
            try:
                command(["docker", "rm", "--force", "--volumes", self.name])
            except Exception as error:
                failures.append(str(error))
        return failures


# [spec:pgorm:req:security.sqlmap.execution]
def interpret(report, log, returncode, target, requests):
    """The sole result adapter for pins.json's CLI --report-json schema."""
    findings = []
    failures = []
    report = report if isinstance(report, dict) else {}
    data, errors = report.get("data"), report.get("error")
    malformed = not isinstance(data, list) or not isinstance(errors, list)
    for entry in data if isinstance(data, list) else []:
        if not isinstance(entry, dict):
            malformed = True
        elif entry.get("type_name") == "TECHNIQUES":
            if isinstance(entry.get("value"), list):
                findings.extend(entry["value"])
            else:
                malformed = True
    errors = [e for e in errors if not isinstance(e, str) or not e.startswith(CLEAN_MESSAGE)] if isinstance(errors, list) else []
    if returncode != 0:
        failures.append("scanner timed out or cancelled" if returncode is None else f"scanner exited {returncode}")
    if report.get("success") is not True:
        failures.append("scanner result missing or unsuccessful")
    if malformed:
        failures.append("malformed scanner data")
    meta = report.get("meta")
    reported = meta.get("url") if isinstance(meta, dict) else None
    try:
        actual, wanted = urllib.parse.urlsplit(reported), urllib.parse.urlsplit(target)
        for field in ("scheme", "hostname", "port", "path"):
            if not reported or getattr(actual, field) != getattr(wanted, field):
                failures.append(f"target {field} mismatch")
    except (ValueError, TypeError, AttributeError):
        failures.append("malformed scanner URL")
    if requests <= 0:
        failures.append("inactive route: no accounted requests")
    valid_findings = [f for f in findings if isinstance(f, dict) and f.get("parameter") == "input" and f.get("place") == "GET" and isinstance(f.get("data"), list) and bool(f["data"]) and all(isinstance(d, dict) and d.get("technique") in TECHNIQUES.values() for d in f["data"])]
    if len(valid_findings) != len(findings):
        failures.append("unexpected injection parameter or malformed finding")
    # sqlmap writes its JSON in finally, including on exceptions. Require a tested
    # parameter terminal message as well as a completed process and valid JSON.
    tested = "parameter 'input' does not seem to be injectable" in log or bool(valid_findings)
    transport = any(s in log.lower() for s in ("connection timed out", "unable to connect", "connection reset", "user aborted", "skipping parameter"))
    if errors:
        failures.append("scanner reported errors")
    if transport:
        failures.append("transport failure or skipped parameter")
    if not tested or "[*] ending @" not in log:
        failures.append("missing completion evidence")
    return {"complete": not failures, "reason": "; ".join(failures) if failures else "completed", "findings": findings, "errors": errors, "requests": requests, "returncode": returncode}


# [spec:pgorm:req:security.sqlmap.outcomes]
def verdict(control, protected, technique, baseline=True, invariant=True):
    if protected.get("findings") or not invariant:
        return "vulnerable"
    if not baseline:
        return "incomplete"
    if not control.get("complete"):
        return "incomplete"
    detected = any(isinstance(f, dict) and f.get("parameter") == "input" and f.get("place") == "GET" and any(isinstance(d, dict) and d.get("technique") == TECHNIQUES[technique] for d in f.get("data", [])) for f in control.get("findings", []))
    if not detected:
        return "invalid-control"
    if not protected.get("complete"):
        return "incomplete"
    return "pass"


# [spec:pgorm:req:security.sqlmap.verdict]
def aggregate(expected, results, cleanup):
    return bool(expected) and len(expected) == len(set(expected)) and set(expected) == set(results) and all(r["outcome"] == "pass" for r in results.values()) and not cleanup


# [spec:pgorm:req:security.sqlmap.profiles]
def inventory(manifest, profile, subset):
    cases = {c["id"]: c for c in manifest["cases"]}
    selected = profile["cases"]
    if subset:
        if any(c not in selected for c in subset):
            raise ValueError("subset contains cases outside the selected profile")
        selected = subset
    if not selected or len(selected) != len(set(selected)):
        raise ValueError("empty or duplicated case inventory")
    return [(cases[c], t) for c in selected for t in cases[c]["techniques"] if t in profile["techniques"]]


def scan(fixture, case, technique, mode, profile, script, artifacts):
    try:
        return scan_inner(fixture, case, technique, mode, profile, script, artifacts)
    except Exception as error:
        output = artifacts / f"{case['id']}-{technique}-{mode}"
        output.mkdir(exist_ok=True)
        try:
            report = read_json(output / "scanner.json")
        except (OSError, ValueError):
            report = None
        result = interpret(report, "", None, "", 0)
        result["reason"] = f"{type(error).__name__}: {error}"
        write_json(output / "result.json", result)
        return result


def scan_inner(fixture, case, technique, mode, profile, script, artifacts):
    key = f"{case['id']}-{technique}-{mode}"
    output = artifacts / key
    output.mkdir()  # Existing scan/session state is an error.
    target = f"{fixture.url}/case/{mode}/{case['id']}?" + urllib.parse.urlencode({"input": case["baseline"]})
    counts_before = http(fixture.url + "/counts")[1].get(f"{mode}/{case['id']}", 0)
    args = [sys.executable, str(script), "--url", target, "-p", "input", "--dbms", "PostgreSQL", "--batch", "--flush-session", "--fresh-queries", "--ignore-proxy", "--disable-coloring", "--technique", technique, "--level", str(profile["level"]), "--risk", str(profile["risk"]), "--threads", "1", "--retries", "0", "--timeout", "15", "--time-sec", "1", "--union-cols", "1-4", "--output-dir", str(output / "session"), "--report-json", str(output / "scanner.json"), "--answers", "extending=N,include=N,fuzzy=N", "-v", "2"]
    for option in ("prefix", "suffix"):
        if option in case:
            args.extend(["--" + option, case[option]])
    write_json(output / "command.json", args)
    started = time.monotonic()
    with (output / "scanner.log").open("w") as log:
        process = subprocess.Popen(args, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        try:
            returncode = process.wait(timeout=profile["case_timeout_seconds"])
        except subprocess.TimeoutExpired:
            stop_process(process)
            returncode = None
        except BaseException:
            stop_process(process)
            raise
    counts = http(fixture.url + "/counts")[1].get(f"{mode}/{case['id']}", 0) - counts_before
    try:
        report = read_json(output / "scanner.json")
    except (OSError, ValueError):
        report = None
    result = interpret(report, (output / "scanner.log").read_text(), returncode, target, counts)
    result["seconds"] = round(time.monotonic() - started, 3)
    write_json(output / "result.json", result)
    return result


# [spec:pgorm:req:security.sqlmap.artifacts]
def source_identity():
    paths = command(["git", "ls-files", "-co", "--exclude-standard"], cwd=ROOT).splitlines()
    files = {p: digest(ROOT / p) for p in sorted(set(paths)) if (ROOT / p).is_file() and not p.startswith((".nplan/", "plan/"))}
    return {"head": command(["git", "rev-parse", "HEAD"], cwd=ROOT), "files": files,
            "digest": hashlib.sha256(json.dumps(files, sort_keys=True).encode()).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", choices=("smoke", "full"), default="smoke")
    parser.add_argument("--case", action="append", default=[])
    parser.add_argument("--artifacts", type=Path, default=ROOT / "target/sqlmap" / time.strftime("%Y%m%d-%H%M%S"))
    parser.add_argument("--baseline-only", action="store_true", help="adapter diagnostic; never a suite pass")
    args = parser.parse_args()
    args.artifacts = args.artifacts.resolve()
    args.artifacts.mkdir(parents=True, exist_ok=False)
    report = {"profile": args.profile, "subset": args.case, "results": {}, "cleanup_errors": [], "pass": False}
    fixture = None
    def interrupted(signum, frame):
        raise KeyboardInterrupt(f"signal {signum}")
    signal.signal(signal.SIGTERM, interrupted)
    try:
        pins = read_json(HERE / "pins.json")
        manifest = read_json(HERE / "cases.json")
        profile = read_json(HERE / "profiles.json")[args.profile]
        work = inventory(manifest, profile, args.case)
        if args.baseline_only:
            work = list({c["id"]: (c,t) for c,t in work}.values())
        report.update(pins=pins, source=source_identity(), manifest_sha256=digest(HERE/"cases.json"), profile_sha256=digest(HERE/"profiles.json"), expected=[f"{c['id']}-{t}" for c,t in work])
        report["results"] = {key: {"outcome": "incomplete", "reason": "not started"} for key in report["expected"]}
        write_json(args.artifacts / "report.json", report)
        cache = ROOT / "target/sqlmap-cache"
        cache.mkdir(parents=True, exist_ok=True)
        script = scanner(pins, cache)
        command(["cargo", "build", "--locked", "--manifest-path", str(HERE / "adapter/Cargo.toml")])
        fixture = Fixture(pins, args.artifacts)
        fixture.start()
        report["database"] = fixture.settings
        for case, technique in work:
            key = f"{case['id']}-{technique}"
            print(f"{key}: baseline", flush=True)
            baselines = {}
            for mode in ("control", "protected"):
                url=f"{fixture.url}/case/{mode}/{case['id']}?" + urllib.parse.urlencode({"input":case["baseline"]})
                try:
                    status, body = http(url)
                    baselines[mode] = {"status":status, "body":body}
                except Exception as error:
                    baselines[mode] = {"status":None, "body":{}, "error":str(error)}
            baseline_ok = all(b["status"] == 200 and b["body"].get("invariant") is True for b in baselines.values())
            if args.baseline_only:
                report["results"][key] = {"outcome":"incomplete", "reason":"baseline diagnostic" if baseline_ok else "failed baseline", "baselines":baselines}
            else:
                control = scan(fixture,case,technique,"control",profile,script,args.artifacts)
                protected_result = scan(fixture,case,technique,"protected",profile,script,args.artifacts)
                try:
                    events = [json.loads(line) for line in (args.artifacts/"requests.jsonl").read_text().splitlines()]
                except (OSError, ValueError) as error:
                    events = []
                    protected_result["complete"] = False
                    protected_result["reason"] = f"request evidence unreadable: {error}"
                invariant = not any(e["route"] == f"protected/{case['id']}" and e["response"].get("invariant") is False for e in events)
                report["results"][key] = {"outcome":verdict(control,protected_result,technique,baseline_ok,invariant),"control":control,"protected":protected_result,"baselines":baselines}
            print(f"{key}: {report['results'][key]['outcome']}",flush=True)
            write_json(args.artifacts/"report.json",report)
    except BaseException as error:
        report["error"] = f"{type(error).__name__}: {error}"
        print(report["error"], file=sys.stderr)
    finally:
        if fixture:
            report["cleanup_errors"] = fixture.close()
        report["pass"] = not args.baseline_only and not report.get("error") and aggregate(report.get("expected",[]),report["results"],report["cleanup_errors"])
        write_json(args.artifacts / "report.json",report)
        summary = f"{args.profile}{' subset' if args.case else ''}: {'PASS' if report['pass'] else 'FAIL'}\n" + "\n".join(f"{k}: {v['outcome']}" for k,v in report["results"].items()) + "\n"
        (args.artifacts/"summary.txt").write_text(summary)
        print(summary,flush=True)
    return 0 if report["pass"] else 1


if __name__ == "__main__":
    sys.exit(main())
