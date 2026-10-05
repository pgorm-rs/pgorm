# sqlmap identifier positions at level 5 — 2026-10-05

**Of the 66 identifier case-technique pairs, 28 have a control sqlmap detects at
level 5 / risk 3, up from 8 at the full profile's level 3.** The new
`identifiers` profile scans the eleven identifier-position cases with every
technique at sqlmap's highest strength. It was run twice with no exemption in
force, and the two runs agreed on every one of the 66 pairs. No protected route
yielded a finding in either run. The 20 pairs that newly fire are now scheduled
by the identifiers profile. The 38 that do not keep their exemptions, whose
evidence now names the level-5 boundaries that were tried.

This is a measurement, not an acceptance run. The measured manifest had the
eleven cases' exemptions removed, so each run scanned all 66 pairs and failed by
design on the 38 controls level 5 cannot reach. The committed manifest schedules
the 28 that fire.

## Why measure it

The full profile scans at level 3 / risk 2, and `--answers extending=N` keeps
sqlmap from raising that during a scan. Its exemption evidence said some
identifier slots were unreachable only because of that ceiling. Of
`stored-identifier`, for example, it said "only the level-5 clause-8 identifier
boundary rewrites the slot into a truth context". Other exemptions made the
opposite claim, that no boundary at any level could reach the slot. Until this
week neither kind had been tested, because no profile ran above level 3, and
until `77528abb` a profile's declared settings did not all reach the scanner.

## Result

Detected in both runs (D), in neither (·). No pair was detected in one counted
run and not the other.

| case | B | E | U | S | T | Q | scheduled before → after |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `pipeline-projection` | · | D | · | · | D | · | 0 → 2 |
| `column` | · | D | · | · | D | · | 0 → 2 |
| `group` | · | D | · | · | D | · | 0 → 2 |
| `stored-identifier` | · | D | · | · | D | · | 0 → 2 |
| `schema` | · | · | · | · | · | · | 0 → 0 |
| `function` | · | · | · | · | · | · | 0 → 0 |
| `graph-alias` | D | D | D | D | D | · | 2 → 5 |
| `table` | D | D | D | D | D | · | 2 → 5 |
| `alias` | D | D | D | D | D | · | 2 → 5 |
| `order` | D | D | · | D | D | · | 1 → 4 |
| `enum-ddl` | · | · | · | D | · | · | 1 → 1 |
| **total** | 4 | 8 | 3 | 5 | 8 | 0 | **8 → 28** |

| | run 1 | run 2 |
| --- | --- | --- |
| controls detected | 28 | 28 |
| controls not detected | 38 | 38 |
| protected routes yielding a finding | **0** | **0** |
| scans complete | 132 / 132 | 132 / 132 |
| infrastructure failures | 0 | 0 |
| cleanup errors | none | none |
| `source_unchanged` | true | true |
| wall time | 26.3 min | 26.4 min |

"Before" is what the full profile schedules among these 66 pairs at level 3,
and it is unchanged. "After" is what the identifiers profile schedules.

## What level 5 reaches, and how

The request evidence names the boundary behind every new detection, and it is
one of two:

- **Clause 8, `"="[ORIGINAL]"`.** At a leading projected identifier it writes
  `SELECT "c"="c" AND <payload> AND "c"="c" FROM items`. The old analysis called
  this slot `SELECT (text AND bool)`, a type error. It is not one: `"c"="c"` is a
  boolean, the projection parses, and the payload is evaluated. Error-based and
  time-based detection fire on `pipeline-projection`, `column`, `group` and
  `stored-identifier`. In `ORDER BY "c"` the same boundary makes the sort key a
  boolean expression, and B, E and T fire on `order`.
- **Clause 9, `" WHERE [RANDNUM]=[RANDNUM]`** with a comment suffix. After a
  FROM or alias tail it opens a WHERE clause, and B, E and T fire on
  `graph-alias`, `table` and `alias`. The old evidence for these pairs said no
  boundary at any level could do that. That was wrong.

The old evidence was also wrong in the other direction. It expected the clause-8
boundary to rescue boolean-blind at a leading projection, and it does not. The
rewrite parses, but it projects `t` where the original page projected names, and
boolean-blind confirms only when its true page matches the original.

PostgreSQL's own error messages, in each run's `requests.jsonl`, explain the 38
pairs that stay out of reach:

- **Leading projection, B/U/S.** B as above. U and S carry their own comment,
  which replaces every closer's suffix and deletes `FROM items`: `column "name"
  does not exist`.
- **`schema`, all six.** Clause 8 is `syntax error at or near "="` between the
  schema and `.items`. Every commenting closer leaves `FROM "fixture"`:
  `relation "fixture" does not exist`.
- **`function`, all six.** The slot is unquoted, so every double-quote closer
  opens an `unterminated quoted identifier`. The parenthesis and empty closers
  leave a syntax error or an unknown column.
- **`enum-ddl` B/E/U/T.** Inside a column definition, clause 8 fails at `=`,
  clause 9 at `WHERE` and a UNION at `UNION`. Only `");<statement>` attaches,
  and that is S.
- **`order` U.** A UNION after ORDER BY is a syntax error at every closer.
- **Q, all eleven.** Every level-4 and level-5 boundary is `where` 1 or 1,2, so
  level 5 adds no REPLACE boundary.

[CONTEXTS.md section 9](../CONTEXTS.md) records this in full, and the earlier
sections now mark where measurement overturned them.

## What changed in the manifest and the harness

An exemption can now carry a `ceiling`: the scanner level at which a boundary
was measured to reach its context. The runner treats the pair as inapplicable in
a profile below the ceiling and schedules it in a profile at or above it. The
20 newly firing pairs carry `ceiling 5`. So the full profile still exempts them
at level 3, where they cannot fire, and the identifiers profile scans them.
The 38 pairs level 5 does not reach have no ceiling, and their evidence now names
the level-5 boundaries tried and what the server said to each. Their boolean
and error-based declarations cite the `where` values that qualify at level 5 /
risk 3 (`[1, 2, 3]`), not level 3's (`[1, 3]`). `security.sqlmap.profiles` is
at `+3`.

The full profile is unchanged: 117 scheduled pairs, 93 declared inapplicable.
The identifiers profile schedules 28 and declares 38 inapplicable.

## CI

The identifiers profile joins CI on the weekly schedule and the manual trigger,
as its own job and status beside the full profile. The decision rests on its
measured cost. The 28 pairs it schedules took 812 and 814 seconds of scanning in
the two runs, about 13.5 minutes. No scan of a scheduled pair took longer than
45 seconds against the 300-second per-scan deadline. The other 38 pairs were
what made each measurement run take 26 minutes, and a scheduled run does not
scan them. The job allows 60 minutes for the scan and 90 for the job. Pull
requests keep the three-pair smoke gate, which a fourteen-minute scan would
dwarf. `security.sqlmap.ci` is at `+1`.

## Provenance

- Profile `identifiers`: level 5, risk 3, all six techniques, `time_sec 3`,
  concurrency 1, retries 0, 15 s HTTP timeout, 10 s statement timeout, 300 s
  per-scan deadline. Profile `17a552ea…`, the committed file. Manifest
  `0a7722e2…`, the measurement manifest with the eleven cases' exemptions
  removed; the committed manifest differs only in those cases' exemptions.
- Source `e6bb4d1a`, plus the uncommitted measurement changes (the profile, the
  harness accepting its name, the exemptions removed) and an untracked working
  note. Source digest `02b688ca…`, byte-identical at the start and end of each
  run.
- Scanner pinned at `d486742eec47ba96940d35bf2dc176f60868efdd`, interpreter
  3.14.4. Adapter binary `bc650539…`, harness binary `23b528dd…`.
- PostgreSQL 16.13 on aarch64 (image `sha256:477b63de…`), UTF8,
  `search_path = fixture, pg_catalog`, `statement_timeout 10s`,
  `lock_timeout 2s`, non-superuser.
- 132 scanner invocations per run. Evidence in `target/sqlmap-ci/ident-r1/`
  (02:50–03:16 UTC) and `target/sqlmap-ci/ident-r2/` (03:16–03:43 UTC).

## The attempt that does not count

A first run, `target/sqlmap-ci/ident-m1/`, started on a quiet machine, but
another session began a multi-core compile partway through: 18 `rustc`
processes and a load average up to 7. It lost `group-T`, whose control SQL is
byte-identical to `column-T`, which it detected. The scanner reported
"considerable lagging has been detected in connection response(s)" on
`group-T`'s protected scan. Every other pair matched the counted runs. Its
timing verdicts are void, and it is not one of the two runs. Both counted runs
were on an awake machine with no competing compile. The power log records no
sleep between 2026-10-04 20:22 and the end of the second run. The one-minute
load average read between 2.3 and 2.5 at the start and end of each run.

## What this does not claim

It does not claim the 38 pairs. At those positions sqlmap has no control that
fires at any level, so a clean protected route says nothing. They belong to the
identifier render oracle (`tests/identifier_oracle/`,
`[spec:pgorm:req:security.ident-oracle]`), which parses every rendered name
rather than probing with payloads. The oracle covers the identifier positions
behind the 28 as well.

It does not turn the 28 into a claim about identifiers in general. For each of
them a control fires, so the protected route's silence is evidence about that
route and that technique, as it is for every other scheduled pair. It is not
evidence that pgorm quotes every name correctly. That is the oracle's claim.

It does not change what the full profile claims. Its identifier cases are
scanned at level 3, where 8 of these 66 pairs fire, as before. The identifiers
profile is a separate, stronger scan with its own status.
