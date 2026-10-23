//! The names live tests give what they create on a shared server, and the
//! sweep that reclaims a crashed run's leftovers without touching anything a
//! running process could still own (`tests/common/run_scope.rs`).
//!
//! Run the test locally:
//! DATABASE_URL=postgres://postgres:postgres@127.0.0.1:54329 cargo nextest run --test run_scope_tests

#![allow(unused_imports, dead_code)]

pub mod common;

use std::process::{Child, Command};
use std::time::{SystemTime, UNIX_EPOCH};

use common::run_scope::{
    self, Creator, config, creator, drop_database, elapsed_secs, name_for, scoped_name, this_run,
};
use pgorm::{ConnectionTrait, DatabasePool, Error, types::ToSql};

fn base_url() -> String {
    dotenvy::from_filename(".env.local").ok();
    dotenvy::from_filename(".env").ok();
    std::env::var("DATABASE_URL").expect("Environment variable 'DATABASE_URL' not set")
}

fn unix_now() -> u32 {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("the clock is past 1970")
        .as_secs();
    u32::try_from(secs).expect("the clock is before 2106")
}

/// A process that runs for the length of the test, killed and reaped however
/// the test ends.
struct Sleeper(Child);

impl Sleeper {
    fn start() -> Self {
        Self(
            Command::new("sleep")
                .arg("60")
                .spawn()
                .expect("sleep is spawned"),
        )
    }
}

impl Drop for Sleeper {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

/// The pid of a process that has exited and been reaped.
fn exited_pid() -> u32 {
    let mut child = Command::new("true").spawn().expect("true is spawned");
    child.wait().expect("true exits");
    child.id()
}

async fn exists(maintenance: &DatabasePool, catalogue: &str, name: &str) -> Result<bool, Error> {
    let sql = match catalogue {
        "database" => "SELECT EXISTS (SELECT 1 FROM pg_database WHERE datname = $1)",
        _ => "SELECT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = $1)",
    };
    let params: [&(dyn ToSql + Sync); 1] = [&name];
    Ok(maintenance
        .get()
        .await?
        .query_one(sql, &params)
        .await?
        .get(0))
}

// [spec:pgorm:req:sql.target+1/test]    a name fits 63 bytes, keeps the
// test's label readable and records the process that minted it
#[test]
fn a_scoped_name_fits_and_records_its_run() {
    let label = "a_label_long_enough_that_no_tail_could_ever_fit_beside_it_whole";
    let name = scoped_name(label);
    assert!(name.len() <= 63, "{name} is {} bytes", name.len());
    assert!(name.starts_with("a_label_long_enough"), "{name}");
    assert_eq!(creator(&name), Some(this_run()));

    let again = scoped_name(label);
    assert_ne!(name, again, "one label asked for twice names two objects");
    assert_eq!(creator(&again), Some(this_run()));

    let short = scoped_name("short");
    assert!(short.starts_with("short_"), "{short}");
    assert_eq!(creator("short"), None);
    assert_eq!(
        creator("crud_tests_5e2d1a7f"),
        None,
        "the checkout-hash form"
    );
}

// [spec:pgorm:req:sql.target+1/test]    ps's elapsed time is read in each of
// its forms, and anything else is not an answer
#[test]
fn elapsed_time_reads_each_ps_form() {
    assert_eq!(elapsed_secs("00:05"), Some(5));
    assert_eq!(elapsed_secs("01:02:03"), Some(3_723));
    assert_eq!(elapsed_secs("2-01:00:00"), Some(2 * 86_400 + 3_600));
    assert_eq!(elapsed_secs("5"), None);
    assert_eq!(elapsed_secs("1:2:3:4"), None);
    assert_eq!(elapsed_secs("ps: no such process"), None);
}

// [spec:pgorm:req:sql.target+1/test]    pgorm-migration mints and sweeps with
// a copy of the helper, which must stay the same file or the two crates' runs
// would stop agreeing on what a name records
#[test]
fn the_migration_copy_matches_this_one() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
    let read = |path: &str| {
        std::fs::read_to_string(root.join(path)).unwrap_or_else(|err| panic!("{path}: {err}"))
    };
    assert!(
        read("tests/common/run_scope.rs") == read("pgorm-migration/tests/common/run_scope.rs"),
        "pgorm-migration/tests/common/run_scope.rs differs from tests/common/run_scope.rs"
    );
}

// [spec:pgorm:req:sql.target+1/test]    the sweep drops a database and a role
// only when their creator is gone from this host and nobody is connected; a
// running creator, one of another host or user, and an occupied database all
// keep theirs
#[pgorm_macros::test]
async fn the_sweep_reclaims_only_what_gone_runs_left() -> Result<(), Error> {
    let base_url = base_url();
    let maintenance = pgorm::connect(config(&base_url, "postgres"));
    let run = this_run();
    let sleeper = Sleeper::start();
    let before_exit = unix_now();
    let gone = Creator {
        pid: exited_pid(),
        stamp: before_exit - 10,
        ..run
    };
    let running = Creator {
        pid: sleeper.0.id(),
        stamp: unix_now() + 1,
        ..run
    };
    let reused = Creator {
        pid: sleeper.0.id(),
        stamp: before_exit - 100,
        ..run
    };
    let elsewhere = Creator {
        owner: run.owner ^ 1,
        ..gone
    };

    let reclaimed = [
        name_for("sweep_gone", gone),
        name_for("sweep_reused", reused),
    ];
    let kept = [
        scoped_name("sweep_this_run"),
        name_for("sweep_running", running),
        name_for("sweep_elsewhere", elsewhere),
    ];
    let occupied = name_for("sweep_occupied", gone);
    for name in reclaimed.iter().chain(&kept).chain([&occupied]) {
        run_scope::create_database(&base_url, name).await?;
    }
    let gone_role = name_for("sweep_gone_role", gone);
    let running_role = name_for("sweep_running_role", running);
    let conn = maintenance.get().await?;
    conn.batch_execute(&format!(
        "CREATE ROLE \"{gone_role}\"; CREATE ROLE \"{running_role}\""
    ))
    .await?;
    let visitor = pgorm::connect(config(&base_url, &occupied));
    let visit = visitor.get().await?;

    run_scope::sweep(&maintenance).await?;

    // Every expectation is read before anything is cleaned up, and the
    // cleanup runs whatever they say: the database recorded for another host
    // is one no sweep will ever reclaim.
    let mut wrong = Vec::new();
    for name in &reclaimed {
        if exists(&maintenance, "database", name).await? {
            wrong.push(format!("{name} survived"));
        }
    }
    for name in kept.iter().chain([&occupied]) {
        if !exists(&maintenance, "database", name).await? {
            wrong.push(format!("{name} was dropped"));
        }
    }
    if exists(&maintenance, "role", &gone_role).await? {
        wrong.push(format!("{gone_role} survived"));
    }
    if !exists(&maintenance, "role", &running_role).await? {
        wrong.push(format!("{running_role} was dropped"));
    }

    drop((visit, visitor));
    for name in kept.iter().chain([&occupied]) {
        drop_database(&base_url, name).await?;
    }
    conn.batch_execute(&format!(
        "DROP ROLE IF EXISTS \"{gone_role}\"; DROP ROLE IF EXISTS \"{running_role}\""
    ))
    .await?;
    drop(sleeper);
    assert!(wrong.is_empty(), "{wrong:#?}");
    Ok(())
}
