//! The one document the subject writes: an executor-shaped report on stdout.

use serde_json::{Value as Json, json};

use crate::{ObservedError, observe};

/// One scheduled effect and what it produced.
#[derive(Clone, Debug)]
struct Step {
    id: String,
    operation: String,
    status: &'static str,
    native_paths: Vec<String>,
    observation: Json,
}

impl Step {
    fn encode(&self) -> Json {
        json!({
            "id": self.id,
            "operation": self.operation,
            "status": self.status,
            "native_paths": self.native_paths,
            "observation": self.observation,
        })
    }
}

/// The subject's accumulated report for one program.
///
/// `builds` is always zero and `cleanup_errors` always empty: the binary is
/// handed a database and a fixture, so it neither compiles anything nor owns
/// resources whose teardown could fail independently of a step.
// [spec:pgorm:req:generative.replay]
#[derive(Clone, Debug)]
pub struct Report {
    program_sha256: String,
    steps: Vec<Step>,
}

impl Report {
    /// Start a report for the program with this digest.
    pub fn new(program_sha256: &str) -> Self {
        Self {
            program_sha256: program_sha256.to_owned(),
            steps: Vec::new(),
        }
    }

    /// Record a step that ran and produced an observation.
    ///
    /// `operation` is the effect's catalog op — `fetch`, `execute`, `inspect`
    /// and so on. The Python side keys its inspection probe off it, so a step
    /// that omitted it would silently drop out of that check.
    pub fn observed(
        &mut self,
        id: &str,
        operation: &str,
        native_paths: &[&str],
        observation: Json,
    ) {
        self.push(id, operation, "observed", native_paths, observation);
    }

    /// Record a step that failed, keeping the classified cause as its observation.
    pub fn failed(
        &mut self,
        id: &str,
        operation: &str,
        native_paths: &[&str],
        error: &ObservedError,
    ) {
        self.push(id, operation, "error", native_paths, observe::error(error));
    }

    /// `"error"` if any step failed, else `"executed"`.
    pub fn status(&self) -> &'static str {
        if self.steps.iter().any(|step| step.status == "error") {
            "error"
        } else {
            "executed"
        }
    }

    /// How many steps have been recorded.
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    /// Whether no step has been recorded yet.
    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// The full report document.
    pub fn to_json(&self) -> Json {
        json!({
            "program_sha256": self.program_sha256,
            "status": self.status(),
            "steps": self.steps.iter().map(Step::encode).collect::<Vec<_>>(),
            "cleanup_errors": [],
            "builds": 0,
        })
    }

    /// Write the report to stdout as exactly one line of JSON.
    // The contract in REPLAY.md is that the subject's whole output is one
    // report on stdout; this is the single site the crate-wide deny allows.
    #[allow(clippy::print_stdout)]
    pub fn emit(self) {
        let document = self.to_json();
        let encoded = serde_json::to_string(&document).unwrap_or_else(|error| {
            json!({
                "program_sha256": self.program_sha256,
                "status": "error",
                "steps": [],
                "cleanup_errors": [{
                    "kind": "error",
                    "class": "InternalError",
                    "cause": error.to_string(),
                    "sqlstate": Json::Null,
                }],
                "builds": 0,
            })
            .to_string()
        });
        println!("{encoded}");
    }

    fn push(
        &mut self,
        id: &str,
        operation: &str,
        status: &'static str,
        native_paths: &[&str],
        observation: Json,
    ) {
        self.steps.push(Step {
            id: id.to_owned(),
            operation: operation.to_owned(),
            status,
            native_paths: native_paths.iter().map(|path| (*path).to_owned()).collect(),
            observation,
        });
    }
}
