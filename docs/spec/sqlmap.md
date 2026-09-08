# External SQL injection verification

This chapter specifies the test harness that drives sqlmap against pgorm on
PostgreSQL. The WBS root is `sqlmap`. Its implementation is test infrastructure;
the SQL rendering fixes remain separate work.

The external scanner supplies independent attack generation and detection.
The direct regressions in `tests/sql_security_tests.rs` establish exact value,
identifier and affected-row behavior. Both are required evidence.

This campaign varies attack inputs to fixed adapter queries. Generated pgorm
operation programs are specified separately in [generative.md](generative.md),
using the public [Python API](python.md). Reusing sqlmap payloads in that
generator does not run the external scanner or satisfy this chapter's verdict.
The Python package and generated campaign do not depend on an HTTP scan passing.

## Scope and fixtures

> [spec:pgorm:def:security.sqlmap]
> The harness consists of a test-only HTTP adapter over actual pgorm APIs,
> disposable PostgreSQL fixtures, an external sqlmap process, and a runner
> that evaluates and records the result. It MUST NOT add a server, sqlmap,
> or scanner-only dependencies to the shipped library's runtime surface.
> Deliberately vulnerable controls MUST exist only in the test harness.
> Scanner findings identify defects; they MUST NOT trigger automatic changes
> to library implementation or the WBS.

> [spec:pgorm:req:security.sqlmap.isolation]
> Each run MUST provision disposable PostgreSQL state and bind its adapter
> to loopback or a private test network. The scanner's target MUST be the
> adapter provisioned by that run, never an arbitrary supplied URL. Requests
> execute with a non-superuser fixture role that cannot create roles or
> databases, execute server programs, or read/write server files. Provisioning
> credentials MUST NOT be exposed to the adapter or scanner. Controls use
> harmless fixture-local effects and require no external network services.
> Every scan has bounded duration; cancellation MUST terminate child
> processes and clean up provisioned state. Cleanup failure is recorded as
> an infrastructure failure rather than a successful run.

> [spec:pgorm:req:security.sqlmap.fixtures]
> Fixtures MUST include matching and nonmatching rows, two distinguishable
> tenants, rows that a guarded write must preserve, same-named objects in
> different schemas, and quoted identifiers and enum types. The runner MUST
> establish a known baseline before each scan case. Write routes MUST reset
> their mutable fixtures before each request, execute the request, and
> report its effects before restoring isolation; controls use the same
> reset policy. Rollback alone MUST NOT be the containment boundary for
> injected SQL. The recorded PostgreSQL version, image identity and relevant
> settings MUST include server encoding, search_path, string-conformance
> mode, statement_timeout and lock_timeout. The normal profile uses UTF-8
> and standard_conforming_strings=on. Any alternate mode is a separate,
> explicitly identified profile, never an unrecorded environment variation.

## Adapter and coverage

> [spec:pgorm:req:security.sqlmap.matrix]
> A version-controlled manifest MUST enumerate the expected scan cases.
> Every case has a stable id, HTTP route/method and input field, pgorm API,
> SQL context, baseline inputs and expected result, applicable scan
> techniques, and its required vulnerable control ids. It also declares
> the names/types and SQL fragments fixed by the fixture. The full profile
> MUST cover every family in the table below; families that contain several
> named contexts require distinct cases where their builders differ.
> The manifest MUST distinguish data parameters, identifiers, typed
> structural inputs and intentionally raw SQL. A protected route MUST NOT
> accept arbitrary SQL fragments. If a typed API cannot express arbitrary
> input, the case records that boundary and checks explicit rejection;
> silently omitting the case does not establish coverage.

| Family | Required contexts |
| --- | --- |
| CRUD values | SELECT predicates, INSERT values, UPDATE values and guards, DELETE guards |
| Composed predicates | Conjoined tenant restriction plus disjunction; empty membership and empty disjunction |
| Graph | Root/joined filters and source aliases through graph execution |
| Pipeline | Literal-value and binder-value paths, filters and projections, joined select_sources |
| Object names | Schema, table, column and alias identifiers; ORDER BY and GROUP BY column selection |
| Type/function names | Function calls, cast targets, qualified enum casts and enum-backed table DDL |
| Parameter substitution | Owned templates containing comments, tagged/untagged dollar strings, quoted identifiers and repeated placeholders |
| Pattern matching | Literal contains/starts_with/ends_with behavior and explicit LIKE patterns |
| Typed structure | Sort direction and LIMIT/OFFSET inputs with explicit parsing boundaries |
| Stored inputs | Values stored through pgorm, read back, then used in a second query through a supported value or identifier API |

> [spec:pgorm:req:security.sqlmap.adapter]
> Protected routes MUST call the named pgorm API from the manifest. The HTTP
> layer MAY decode transport syntax and parse inputs into the types the API
> requires; it MUST NOT apply SQL escaping, SQLi wordlists, payload filtering,
> or replace a literal-value path with a bound-value path. Arbitrary strings
> and identifiers reach the selected API unchanged after transport decoding.
> Boolean directions and numeric limits use the real typed API; invalid
> representations are explicit request rejections. Owned raw-query routes
> vary only their declared input slots, never the entire SQL template.
> Adapter routes and dependencies remain test-only, including any HTTP
> framework. The implementation MUST register its actual source and test
> paths with nspec so harness implementation and verification are tracked
> without moving code into the library merely to obtain coverage.

> [spec:pgorm:req:security.sqlmap.controls]
> Each protected case MUST reference one or more deliberately vulnerable
> controls with the same transport, input context, fixtures and observable
> result channel. Controls MUST cover each injection technique enabled for
> that case, using separate controls when necessary. They MUST remain
> vulnerable when the ORM is fixed and MUST NOT call the escaping path they
> are intended to validate. A control is successful only when sqlmap reports
> the expected injection point and technique; an HTTP error or a changed
> response is not sufficient. The runner MUST test the controls in the same
> run and with the same profile as their protected cases.

> [spec:pgorm:req:security.sqlmap.observability]
> Before scanning, every case MUST pass a baseline check proving that its
> adapter route is reachable, invokes the intended API and produces the
> expected response. Accepted inputs that change the query result MUST
> produce distinguishable responses. Responses expose deterministic row
> content/counts or affected-row and protected-sentinel outcomes as declared
> by the case, without timestamps or unrelated dynamic content. Harness
> errors, expected invalid-input rejections and PostgreSQL errors remain
> distinguishable. A server error MUST NOT be converted into an ordinary
> empty-result response. Route instrumentation MUST account for scan
> requests reaching the declared API, making an inactive adapter observable.

## Scanner execution

> [spec:pgorm:req:security.sqlmap.pin]
> The runner MUST invoke upstream sqlmap as an external program at an exact
> recorded commit, with a content-verified installation or immutable image.
> A moving branch or tag alone is not a pin. Python/runtime dependencies and
> the PostgreSQL fixture image are pinned through a lock or immutable image
> identity. The repository records the upstream URL, revision, content
> identity and applicable license notices. Changing the scanner or payload
> revision requires a reviewed pin update and a fresh control/full-suite
> run; an earlier report cannot stand for the new revision.

> [spec:pgorm:req:security.sqlmap.profiles]
> Version-controlled smoke and full profiles MUST declare their exact case
> ids, test parameters, PostgreSQL DBMS selection, techniques, level/risk,
> request concurrency, retry policy and time budgets. Both run
> noninteractively with fresh sqlmap session/output state for every case;
> cached findings from another case or run MUST NOT satisfy a control or
> protected result. The full profile includes every manifest case and
> exercises boolean, error, UNION, stacked, time and inline-query techniques
> where the context supports them. Inapplicability needs an explicit,
> reviewable reason in the manifest. Smoke is an explicitly smaller claim.
> Database and HTTP timeouts MUST allow the profile's expected time-control
> delay, with a documented margin; an outer deadline still bounds the scan.

> [spec:pgorm:req:security.sqlmap.execution]
> The runner MUST inventory expected work before starting, perform baseline
> and control checks, invoke each scheduled scan, and wait for a terminal
> scanner result. It MUST capture process status, scanner findings, scanner
> errors and case-level request evidence through the interface supported by
> the pinned revision. An exit code of zero or absence of a finding alone
> MUST NOT be interpreted as successful completion. Version-specific result
> interpretation belongs to one tested adapter. Failed controls MAY abort
> dependent work, but every remaining case is then explicitly incomplete.

## Verdict and evidence

> [spec:pgorm:req:security.sqlmap.outcomes]
> Each expected case MUST end in exactly one outcome: `pass`, `vulnerable`,
> `invalid-control`, or `incomplete`, with a reason and supporting evidence.
> `vulnerable` means an injection was detected on a protected route or a
> declared data/state invariant failed. `invalid-control` means an expected
> vulnerable control was not detected as required. `incomplete` includes
> failed baselines, missing routes, unaccounted/skipped work, timeouts,
> crashes, transport failures and missing or unreadable scanner output.
> An expected invalid-input rejection is recorded separately inside its
> case and is not itself an infrastructure failure or evidence of SQLi.
> Whether a case passes MUST depend on its completed scan, controls and
> invariants, not on the HTTP status of an individual attack request.

> [spec:pgorm:req:security.sqlmap.verdict]
> An aggregate run passes if and only if all expected cases are accounted
> for, all baselines and required controls succeed, every protected case
> passes, and fixture cleanup succeeds. Detected vulnerabilities,
> invalid controls and incomplete work MUST each make the command exit
> nonzero. Empty discovery MUST fail. A subset run MUST name its subset and
> MUST NOT be reported as a full-suite pass. Infrastructure failure and
> vulnerability are distinct report outcomes even though both fail the job.

> [spec:pgorm:req:security.sqlmap.runner-tests]
> Runner tests MUST demonstrate non-passing results for an undetected
> positive control, unreachable/inactive route, failed baseline, cancelled
> or timed-out scan, zero-exit process with missing output, malformed
> output, stale session data, skipped case and empty case discovery. They
> MUST also demonstrate that a finding on a protected route fails, an
> expected control finding succeeds, and a fully accounted clean fixture
> passes. These fast tests MUST NOT require a complete live sqlmap campaign.

> [spec:pgorm:req:security.sqlmap.artifacts]
> Every attempted run MUST produce a machine-readable report and a concise
> human summary, including failed and incomplete runs. The report identifies
> the pgorm commit and any dirty-tree changes used, scanner/runtime/database
> identities, manifest and profile digests, effective options/settings,
> expected and executed case ids, outcomes, control detections and timings.
> It retains scanner logs/results and request/API evidence needed to explain
> each result. A finding MUST preserve a replayable fixture-local request or
> payload and identify the pgorm API involved. Artifacts MUST omit credentials
> and use only synthetic fixture data. A documented command reproduces the
> run from the recorded pins, configuration and source revision.

## Integration and acceptance

> [spec:pgorm:req:security.sqlmap.ci]
> CI MUST provide a bounded smoke job for relevant pull requests, a scheduled
> full job and a manually invocable full job. All use the same local runner,
> profiles and verdict logic, retain artifacts on failure, and expose the
> profile's status separately. Scanner work MUST stay out of the existing
> pre-commit check budget. The job's permissions and services MUST be limited
> to the disposable harness. Where external-source pull requests cannot
> provision the harness, the status is explicitly not-run; the integration
> policy must obtain a trusted run rather than treating the skip as a pass.

> [spec:pgorm:req:security.sqlmap.acceptance]
> Initial acceptance requires a passing full-profile report against the
> recorded implementation revision, with all vulnerable controls detected,
> plus passing direct SQL security regressions against the same source and
> database configuration. Any newly discovered failures receive named
> regressions before closure. The report may state that pgorm passes that
> pinned sqlmap integration suite and must identify its manifest/profile;
> it MUST NOT present scanner silence as proof that every possible ORM
> query is injection-free. Implementation fixes remain on the corresponding
> defect nodes; changing or weakening the harness to conceal those defects
> does not satisfy acceptance.

## External references

- [sqlmap usage and automation options](https://github.com/sqlmapproject/sqlmap/wiki/Usage)
- [sqlmap test context and technique metadata](https://github.com/sqlmapproject/sqlmap/blob/master/data/xml/payloads/boolean_blind.xml)
- [sqlmap API implementation](https://github.com/sqlmapproject/sqlmap/blob/master/lib/utils/api.py)

These links explain the upstream interface. The implementation pin and
recorded artifacts, rather than the current contents of a moving upstream
page, identify the scanner used for a particular acceptance run.
