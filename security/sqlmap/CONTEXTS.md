# sqlmap detection contexts — what each technique needs to catch a control

Analysis only. Derived by reading the pinned scanner's payload/boundary
definitions and the harness invocation, cross-referenced against the recorded
`acceptance/2026-09-09.md` outcome (105 pass / 105 invalid-control). No files
were changed and the scanner was not run.

Authoritative sources read:
- `target/sqlmap-cache/scanner/sqlmap-d486742eec47ba96940d35bf2dc176f60868efdd/data/xml/boundaries.xml`
- `.../data/xml/payloads/{boolean_blind,error_based,inline_query,stacked_queries,time_blind,union_query}.xml`
- `.../lib/core/enums.py` (`PAYLOAD.CLAUSE`, `PAYLOAD.WHERE`, `PAYLOAD.TECHNIQUE`)
- `.../lib/controller/checks.py` (the boundary↔payload pairing loop, lines ~403–456; the GREP result path, ~683–700)
- `.../lib/core/agent.py` (`prefixQuery` / `suffixQuery`)
- `security/sqlmap/{profiles.json,cases.json}`, `adapter/src/main.rs` (`control`, line 162)
  and `adapter/src/harness/` (the runner)

---

## 1. The matching model (must be understood before any verdict)

Each `(case, technique)` is an **independent** scan. The harness runs
(`adapter/src/harness/mod.rs`, `scanner_args`):

```
sqlmap --url .../case/control/<id>?input=<baseline> -p input \
       --dbms PostgreSQL --technique <one of B|E|U|S|T|Q> \
       --level 3 --risk 2 --union-cols 1-4 \
       --answers extending=N,include=N,fuzzy=N --batch ...
```

No case in `cases.json` sets `--prefix`/`--suffix`, so sqlmap uses its own
boundary catalogue unmodified.

`checks.py` pairs a **payload** (from `payloads/*.xml`) with a **boundary**
(from `boundaries.xml`) only when *all* of the following hold:

1. **Technique** — `test.stype` equals the single `--technique` letter for this
   scan. (`checks.py:257`. `enums.py`: BOOLEAN=1 B, ERROR=2 E, QUERY=3 Q,
   STACKED=4 S, TIME=5 T, UNION=6 U.)
2. **DBMS** — a `<details><dbms>` restriction must be PostgreSQL; generic
   (no `<dbms>`) tests always qualify. MySQL/Oracle/… tests never fire here.
3. **Test level/risk ceiling** — `test.level ≤ 3` and `test.risk ≤ 2`
   (`checks.py:338,346`).
4. **Boundary level ceiling** — `boundary.level ≤ 3` (`checks.py:409`). The
   `--answers extending=N` reply declines sqlmap's "extend to level 5/risk 3?"
   prompt (`checks.py:192`), so **level 3 is a hard ceiling on boundaries** —
   every level-4 and level-5 boundary is unreachable in this suite.
5. **Clause intersection** — `test.clause ∩ boundary.clause ≠ ∅`, unless either
   side is `[0]` "Always" (`checks.py:416–421`).
6. **Where intersection** — `test.where ∩ boundary.where ≠ ∅`
   (`checks.py:428–433`). `PAYLOAD.WHERE`: 1=ORIGINAL (append to value),
   2=NEGATIVE (invalidate then append), 3=REPLACE (drop value, substitute).

For REPLACE (`where=3`) the prefix and suffix are both dropped — the parameter
value is replaced wholesale by the bare vector (`agent.py` `prefixQuery`:
`if where == REPLACE ... query = ""`; `suffixQuery`: `if where == REPLACE ...
pass`). This single fact governs the entire inline-query story (§5).

`--union-cols 1-4` means UNION column-count is brute-forced 1→4; none of the
controls project more than a handful of columns, so **column count is never the
blocker** — the UNION failures below are all structural, not arity.

Stacked queries are viable because the control executes through
`app.raw.simple_query(&sql)` (`adapter/src/main.rs:188`), i.e. the PostgreSQL
simple-query protocol, which permits multiple `;`-separated statements. (The
protected routes never take that path, which is the point of the suite.)

---

## 2. The boundary catalogue actually available at level 3

Filtering `boundaries.xml` to `<level> ≤ 3` leaves **only** boundaries whose
`ptype ∈ {1,2,3,4,5}` and whose `clause ∈ {0, 1, (1,2,3)}`. Everything that
could escape an identifier, a block comment, a PostgreSQL `$$` string, a
pre-WHERE/`VALUES` position, or a table/column name is level 4–5 and therefore
**out of reach**. The reachable set, by parameter type:

**ptype 1 — unescaped numeric / no quotes** (escapes bare-number and
paren-expression positions):
```xml
<boundary><level>3</level><clause>1</clause><where>1,2</where><ptype>1</ptype>
  <prefix>)</prefix><suffix>[GENERIC_SQL_COMMENT]</suffix></boundary>
<boundary><level>1</level><clause>0</clause><where>1,2,3</where><ptype>1</ptype>
  <prefix></prefix><suffix></suffix></boundary>          <!-- the only where=3 boundary -->
<boundary><level>1</level><clause>1</clause><where>1,2</where><ptype>1</ptype>
  <prefix></prefix><suffix>[GENERIC_SQL_COMMENT]</suffix></boundary>
```
plus the `)`, `))`, `)))` + `AND ([RANDNUM]=[RANDNUM]` variants (levels 1–3) and
`# [RANDSTR]` (level 3).

**ptype 2 — single-quoted string** (escapes `'...'` in a WHERE/HAVING truth
context):
```xml
<boundary><level>3</level><clause>1,2,3</clause><where>1,2</where><ptype>2</ptype>
  <prefix>'</prefix><suffix>[GENERIC_SQL_COMMENT]</suffix></boundary>
<boundary><level>1</level><clause>1</clause><where>1,2</where><ptype>2</ptype>
  <prefix>'</prefix><suffix> AND '[RANDSTR]'='[RANDSTR]</suffix></boundary>
```
plus `')`, `'))`, `')))`, and `'` + `OR '[RANDSTR1]'='[RANDSTR2]` (levels 1–3).

**ptype 3 — LIKE single-quoted string** (`'`, `')`, `'))`, `%'` closers with
`... LIKE '[RANDSTR]` suffixes, levels 2–3).

**ptype 4 / 5 — double-quoted string** (`"`, `")`, `"))` closers, levels 2–3).
In PostgreSQL `"..."` is an **identifier**, so these are the only lever against
a quoted identifier at level 3 — and they only help when the identifier sits at
a position where, after closing the `"`, a set-operation or statement
terminator is legal (see §6, Class B).

**Not available at level 3** (all level 4–5), which is why whole context
classes collapse:
```xml
<!-- Identifier escapes (ptype 6): the natural tool for a quoted identifier -->
<boundary><level>4</level><clause>1</clause>...<ptype>6</ptype><prefix>` WHERE [RANDNUM]=[RANDNUM]</prefix>...
<boundary><level>4</level><clause>8</clause>...<ptype>6</ptype><prefix>`=`[ORIGINAL]`</prefix>...
<boundary><level>5</level><clause>8</clause>...<ptype>6</ptype><prefix>"="[ORIGINAL]"</prefix>...
<!-- Table-name context (clause 7) -->
<boundary><level>5</level><clause>7</clause>...<ptype>6</ptype><prefix> [RANDSTR1],</prefix>...
<!-- Pre-WHERE (clause 9): the natural tool for SET / VALUES positions -->
<boundary><level>4</level><clause>9</clause>...<prefix>' WHERE [RANDNUM]=[RANDNUM]</prefix>...
<boundary><level>5</level><clause>9</clause>...<prefix>') WHERE [RANDNUM]=[RANDNUM]</prefix>...
<!-- ') + comment : the natural tool for a VALUES-list string literal -->
<boundary><level>4</level><clause>1</clause>...<ptype>2</ptype><prefix>')</prefix><suffix>[GENERIC_SQL_COMMENT]</suffix></boundary>
<!-- Block comment (ptype 7) and PostgreSQL $$ (ptype 8) -->
<boundary><level>4</level>...<ptype>7</ptype><prefix>*/</prefix><suffix>/*</suffix></boundary>
<boundary><level>5</level>...<ptype>8</ptype><prefix>$$</prefix><suffix>[GENERIC_SQL_COMMENT]</suffix></boundary>
```

---

## 3. What each technique's payloads demand (clause / where), gated to L≤3, R≤2, PostgreSQL-or-generic

Distinct `(clause, where)` across the qualifying tests in each payload file:

| Tech | payload `(clause, where)` at L≤3/R≤2 | needs a boundary with… |
| --- | --- | --- |
| B | `(1,8,9; w1)`, `(1; w1)`, `(1,2; w1)`, `(1,8; w1)`, `(2,3; w1)`, `(1,2,3; w3)`, `(1,3; w3)` | clause∋1 & where∋1 (append); the w3 tests also ride the `clause 0` boundary |
| E | `(1,8,9; w1)`, `(2,3; w1)`, `(1,2,3,9; w3)` | clause∋1 & where∋1 |
| U | `(1,2,3,4,5; w1)` ×11 | clause∋1 & where∋1; vector `[UNION]`, `<comment>[GENERIC_SQL_COMMENT]` |
| S | `(1; w1)` | clause=1 & where∋1; PostgreSQL variants carry `<comment>--</comment>` |
| T | `(1,2,3,8,9; w1)`, `(2,3; w1)`, `(1,2,3,9; w3)` | clause∋1 & where∋1 |
| Q | `(1,2,3,8; w3)` **only** | **where∋3 — matches the `clause 0` empty boundary alone** |

Every non-Q technique has a `where=1` test whose clause includes `1`, so it can
always pair with the level-3 ptype-2 `'` WHERE boundary (or ptype-4 `"` for
identifiers, or ptype-1 `)`/`` for numeric). **Q is the outlier**: its only
qualifying tests are `where=3` (REPLACE), and the only `where=3` boundary in the
entire catalogue is the empty/empty `clause 0` numeric one.

The PostgreSQL inline-query vector (`inline_query.xml`, the only Q test that
runs under `--dbms PostgreSQL`, level 2):
```xml
<test><title>PostgreSQL inline queries</title><stype>3</stype><level>2</level><risk>1</risk>
  <clause>1,2,3,8</clause><where>3</where>
  <vector>(SELECT '[DELIMITER_START]'||([QUERY])::text||'[DELIMITER_STOP]')</vector>
  <request><payload>(SELECT '[DELIMITER_START]'||(SELECT (CASE WHEN ([RANDNUM]=[RANDNUM]) THEN 1 ELSE 0 END))::text||'[DELIMITER_STOP]')</payload></request>
  <response><grep>[DELIMITER_START](?P<result>.*?)[DELIMITER_STOP]</grep></response>
  <details><dbms>PostgreSQL</dbms></details></test>
```

---

## 4. The Q (inline query) finding — the headline

**Q is detected in 0 / 35, and no *faithful* control can change that.** This is
structural, not a level or tuning gap.

Two independent requirements must both hold, and no pgorm-representative context
satisfies either:

1. **Escape.** Q is `where=3` REPLACE-only. REPLACE drops prefix and suffix
   (`agent.py`), so the entire parameter value is replaced by the bare vector
   `(SELECT '…'||(…)::text||'…')`. The **only** boundary compatible with
   `where=3` is:
   ```xml
   <boundary><level>1</level><clause>0</clause><where>1,2,3</where><ptype>1</ptype>
     <prefix></prefix><suffix></suffix></boundary>
   ```
   It carries **no quote and no comment**. Therefore Q can only land in a
   position where the raw value already sits **unquoted**. Inside `'…'` the
   vector's own quotes collide with the app's quotes → broken SQL; inside `"…"`
   the vector becomes a nonsense identifier. There is no `where=3` boundary that
   closes a `'` or a `"` at *any* level, so raising `--level` would not help.

2. **Reflection.** Q is a GREP technique: `checks.py` performs the request and
   greps the **response body** for `[DELIMITER_START](?P<result>.*?)[DELIMITER_STOP]`,
   confirming only if `result == '1'`. The injected sub-SELECT's *evaluated
   string* must be echoed back. That happens only when the injection is a
   **SELECT/RETURNING projection expression** whose value is returned to the
   client — not a WHERE value (filters, never echoes), not ORDER BY / LIMIT /
   OFFSET (never echoed), not a type/function/identifier name.

Answer to the specific LIMIT question: **`LIMIT` is not reachable for Q, and not
merely "unreached."** `LIMIT (SELECT 1)` is valid PostgreSQL, but (a) the
inline-query vector is `::text`-typed, so `LIMIT (…)::text` is a type error, and
(b) even a well-typed numeric subquery in LIMIT is never reflected, so the grep
can never match. Q needs a *reflected, unquoted projection expression* —
"something else entirely," not a LIMIT.

The minimal shape that *would* let Q fire is the textbook inline-query context —
an unquoted, reflected projection: `SELECT {input} FROM items` (REPLACE →
`SELECT (SELECT '…'||(…)::text||'…') FROM items`, whose delimited value is
returned as a column and grepped). **But pgorm never emits user input into an
unquoted projection position** — values are bound parameters and identifiers are
double-quoted. So an unquoted-projection control would represent a context pgorm
does not have; it is not a faithful positive control. Q is best characterised as
**inapplicable to this suite as constructed** (see §7). Confidence: high.

---

## 5. Matrix — technique × context class

Verdicts: **L3** = reachable at the pinned level 3 (matches an observed pass);
**L≥4** = would need a level-4/5 boundary the profile forbids;
**struct** = structurally impossible in this context at *any* level / any control
faithful to the context. Remedies prefer a conventional injectable shape sqlmap
already ships boundaries for.

| | **Class A — single-quoted string** `'{input}'` | **Class B — double-quoted identifier** `"{input}"` | **Class C — unquoted token** `{input}` |
| --- | --- | --- | --- |
| **B** boolean | **L3** in a WHERE/HAVING truth-context via `'`+`AND '[RANDSTR]'='[RANDSTR]`. **L≥4** for SET/VALUES (needs pre-WHERE clause-9 / `')`+comment). | **struct** at a leading projected identifier; **struct** at a bare FROM/alias/ORDER-BY tail (no truth slot). At best **L≥4/5** via the clause-8 `"="[ORIGINAL]"` identifier boundary. | **L3** for numeric/paren positions via `)` / empty+`AND`. Depends on position. |
| **E** error | **L3** in a WHERE context (same `'` boundary). **L≥4** for SET/VALUES. | Same as B-row: **struct** for leading projection & bare-name tails; else **L≥4**. | mixed: **L3** where a paren closes into a WHERE-like expr; **struct** in ORDER-BY/LIMIT tails. |
| **U** union | **L3** iff the statement is a **SELECT**; **struct** on INSERT/UPDATE/DELETE controls. | **L3** when the identifier is a **trailing** table/alias (`FROM "x"`, `AS "x"`) — close `"`, `UNION SELECT …`, comment. **struct** at a leading projection (comment kills the required `FROM`) and after an ORDER BY. | **struct** in every trailing position (LIMIT/OFFSET/ORDER-BY tail); **L3** only where a `)` reopens a SELECT set-op (e.g. `cast`). |
| **S** stacked | **L3** everywhere the `'` can be closed and the tail commented (SELECT *and* DML). | **L3** when the `"`/`")` closes and a `;stmt` + comment fits (`table`, `order`, `alias`, `enum`, `enum-ddl`). **struct** at a leading projection (first statement loses its FROM → whole batch errors). | **L3** wherever a `)` or bare token can be closed then `;stmt-- ` appended. |
| **T** time | **L3** in a WHERE context. **L≥4** for SET/VALUES. | Same as B/E rows. | mixed, as E-row. |
| **Q** inline | **struct** — REPLACE cannot escape `'…'`; not reflected. | **struct** — REPLACE cannot escape `"…"`; not reflected. | **struct** — type/keyword/number positions are not reflected unquoted projections. |

**Evidence anchors for the cells:**

- Class A / B / E / T reachable — ptype-2 WHERE boundary
  `<prefix>'</prefix><suffix> AND '[RANDSTR]'='[RANDSTR]</suffix>` (`boundaries.xml`,
  level 1, clause 1). Matches every WHERE-context pass (`select`, `delete`,
  `contains`, …).
- Class A / U struct on DML — a UNION needs a SELECT set-operation position; an
  `INSERT`/`UPDATE`/`DELETE … WHERE name='' UNION SELECT …` is a syntax error,
  and `RETURNING` cannot be unioned onto through the WHERE. Matches `insert`,
  `update-value`, `update-guard`, `delete`, `tenant-or`, `empty-in`, `empty-or`
  all showing U invalid.
- Class A / SET & VALUES gated to L≥4 — the pre-WHERE boundaries
  (`<clause>9</clause> <prefix>' WHERE [RANDNUM]=[RANDNUM]</prefix>`, level 4) and
  `<prefix>')</prefix><suffix>[GENERIC_SQL_COMMENT]</suffix>` (level 4) are the
  tools for `SET name='…'` and `VALUES(…,'…',…)`; both exceed level 3.
- Class B / U & S reachable at a trailing identifier — ptype-4 boundary
  `<prefix>"</prefix>` closes the app's `"`; the UNION/stacked payloads bring
  their own `<comment>[GENERIC_SQL_COMMENT]</comment>` / `--`, which
  `suffixQuery` appends *instead of* the boundary's `AND "x"="x"` suffix (the
  `elif suffix and not comment` branch). Matches `table`/`alias` passing U,S.
- Class B / leading-projection struct — for `SELECT "{input}" FROM items`, any
  comment removes ` FROM items` leaving an unresolved column, and any appended
  ` AND "x"="x"` makes `SELECT (text AND bool)` a type error; so B/E/U/S/T/Q all
  fail. Matches `column`, `group`, `pipeline-projection`, `stored-identifier`.
- Class C / U struct after terminal clauses — nothing may follow ORDER
  BY/LIMIT/OFFSET; matches `direction`, `limit`, `offset`, and `order` all
  showing U invalid.

---

## 6. Per-case table — 35 cases mapped to context class

Control SQL is from `adapter/src/main.rs` `control()` (unlisted cases fall to the
`_ =>` default `SELECT name FROM items WHERE name='{input}'`). "Observed" is the
`invalid-control` set from `acceptance/2026-09-09.md`; the complement passed.

| Case | Control shape | Class (sub-shape) | Observed invalid | Primary reason |
| --- | --- | --- | --- | --- |
| `select` | `… WHERE name='{input}'` (default) | A (WHERE) | Q | Q struct |
| `insert` | `INSERT … VALUES (10,1,'{input}','inserted') RETURNING` | A (VALUES, non-final col) | B,E,U,S,T,Q | see §7 — VALUES-middle |
| `update-value` | `UPDATE … SET name='{input}' WHERE id=1` | A (SET / pre-WHERE) | B,E,U,T,Q | SET→L≥4; U struct; Q struct |
| `update-guard` | `UPDATE … WHERE tenant=1 AND name='{input}'` | A (WHERE, UPDATE) | U,Q | U struct (UPDATE); Q struct |
| `delete` | `DELETE … WHERE tenant=1 AND name='{input}'` | A (WHERE, DELETE) | U,Q | U struct (DELETE); Q struct |
| `tenant-or` | `DELETE … name='{input}'` | A (WHERE, DELETE) | U,Q | U struct; Q struct |
| `empty-in` | `DELETE … name='{input}'` | A (WHERE, DELETE) | U,Q | U struct; Q struct |
| `empty-or` | `DELETE … name='{input}'` | A (WHERE, DELETE) | U,Q | U struct; Q struct |
| `graph-root` | default `… WHERE name='{input}'` | A (WHERE) | Q | Q struct |
| `graph-joined` | default `… WHERE name='{input}'` | A (WHERE) | Q | Q struct |
| `graph-alias` | `… FROM items AS "{input}"` | B (trailing alias) | B,E,T,Q | bare-alias tail: only U,S; Q struct |
| `pipeline-literal` | default `… WHERE name='{input}'` | A (WHERE) | Q | Q struct |
| `pipeline-bound` | default `… WHERE name='{input}'` | A (WHERE) | Q | Q struct |
| `pipeline-projection` | `SELECT "{input}" FROM items` | B (leading projection) | B,E,U,S,T,Q | leading-projection struct |
| `pipeline-sources` | default `… WHERE name='{input}'` | A (WHERE) | Q | Q struct |
| `schema` | `SELECT name FROM "{input}".items` | B (schema-qualified) | B,E,U,S,T,Q | `.items` binds schema; see §7 |
| `table` | `SELECT name FROM "{input}"` | B (trailing table) | B,E,T,Q | bare-FROM tail: only U,S; Q struct |
| `column` | `SELECT "{input}" FROM items` | B (leading projection) | B,E,U,S,T,Q | leading-projection struct |
| `alias` | `SELECT name FROM items AS "{input}"` | B (trailing alias) | B,E,T,Q | bare-alias tail: only U,S; Q struct |
| `order` | `SELECT name FROM items ORDER BY "{input}"` | B (ORDER BY tail) | B,E,U,T,Q | ORDER-BY tail: only S; U struct; Q struct |
| `group` | `SELECT "{input}" … GROUP BY "{input}"` | B (leading projection ×2) | B,E,U,S,T,Q | leading-projection struct |
| `function` | `SELECT {input}('Alice')` | C (function name) | B,E,U,S,T,Q | see §7 — callable name |
| `cast` | `SELECT CAST('alice' AS {input})` | C (type in CAST paren) | E,T,Q | `)` gives B,U,S; E/T weak; Q struct |
| `enum` | `SELECT CAST('ready' AS fixture."{input}")::text` | B (qualified type id) | B,E,U,T,Q | `")` gives S; Q struct |
| `enum-ddl` | `CREATE TABLE … (value "{input}"); …` | B (type id, DDL) | B,E,U,T,Q | `")`/`;` gives S; Q struct |
| `parameters` | `SELECT '{input}'::text` | A (projection literal, SELECT) | Q | Q struct |
| `contains` | `… WHERE name LIKE '%{input}%'` | A (LIKE) | Q | Q struct |
| `starts-with` | `… WHERE name LIKE '{input}%'` | A (LIKE) | Q | Q struct |
| `ends-with` | `… WHERE name LIKE '%{input}'` | A (LIKE) | Q | Q struct |
| `like` | `… WHERE name LIKE '{input}'` | A (LIKE) | Q | Q struct |
| `direction` | `… ORDER BY name {input}` | C (ASC/DESC keyword) | U,Q | U struct (after ORDER BY); Q struct |
| `limit` | `… LIMIT {input}` | C (bare integer, tail) | U,Q | U struct (after LIMIT); Q struct |
| `offset` | `… OFFSET {input}` | C (bare integer, tail) | U,Q | U struct (after OFFSET); Q struct |
| `stored-value` | `… WHERE name='{saved}'` | A (WHERE, via stored) | Q | Q struct |
| `stored-identifier` | `SELECT "{saved}" FROM items` | B (leading projection) | B,E,U,S,T,Q | leading-projection struct |

Every observed invalid cell above is explained by exactly one of: **Q struct**
(§4), **U struct** (non-SELECT or post-terminal-clause), **leading-projection
struct** (Class B), a **bare-name tail** admitting only U/S or only S, or a
**level-≥4 gate** (SET/VALUES). None is a mystery or a scanner defect.

---

## 7. Genuinely unreachable — stated plainly

The operator has ruled out declaring inapplicability as an escape hatch, so the
cells that *cannot* be repaired by any faithful control need to be named. Two
tiers:

### 7a. Structurally impossible at any level (no faithful control exists)

- **Q (inline query) — all 35 cases.** REPLACE-only + reflection requirement
  (§4). pgorm binds values and quotes identifiers, so it never produces the
  unquoted reflected projection Q needs. A control that added one would be
  testing a context pgorm does not expose. *No faithful positive control for Q
  exists anywhere in this suite.*
- **U (union) on DML controls** — `insert`, `update-value`, `update-guard`,
  `delete`, `tenant-or`, `empty-in`, `empty-or`. UNION requires a SELECT
  set-operation position; INSERT/UPDATE/DELETE offer none (RETURNING is not
  unionable through the WHERE). The only way to "fix" it is to make the control a
  SELECT — which stops it being an insert/update/delete control.
- **U (union) after a terminal clause** — `order` (ORDER BY), `direction`
  (ORDER BY tail), `limit` (LIMIT), `offset` (OFFSET). Nothing may syntactically
  follow these clauses, so no `UNION SELECT` can attach while the injection point
  stays in that clause.
- **B/E/T on a bare object-name tail** — `table` (`FROM "x"`), `alias` /
  `graph-alias` (`AS "x"`), and `order` (`ORDER BY "x"`): after closing the
  identifier you are in FROM/alias/ORDER-BY position, which has no truth slot to
  attach ` AND <inference>` / an error subquery / a timing subquery. Only set-op
  (U, where legal) and statement-terminator (S) work. This is why these cases
  structurally cap at "U,S" or "S".
- **Leading projected identifier** — `column`, `group`,
  `pipeline-projection`, `stored-identifier` (`SELECT "{input}" FROM items`):
  the injected name is a projected column that *depends on* the trailing
  `FROM items`. Commenting forward deletes the FROM (unresolved column error);
  appending a predicate makes `SELECT (text AND bool)` a type error; UNION/stacked
  lose the FROM on the left statement. All six techniques fail, and only the
  level-5 clause-8 identifier boundary would rescue B alone — so at the pinned
  level 3 this context is a dead zone for *every* technique.
  The only faithful repair is to relocate the identifier to a **trailing** slot
  (the `table`/`order` shape), which changes the case's meaning.
- **`schema`** (`FROM "{input}".items`): the trailing `.items` is bound to the
  injected schema name. Commenting it away leaves `FROM "schema"` (a schema is
  not a relation → error); appending between the identifier and `.items` is a
  syntax error. No level-3-or-higher boundary reconstructs this, so all six miss.
- **`function`** (`SELECT {input}('Alice')`): the injected token is a **callable
  name** immediately followed by `(`. REPLACE/append both yield
  `<expr>('Alice')` — calling an expression/subquery result as a function, which
  is not valid; and there is no boundary that closes out of a function-name slot
  at level 3. All six miss.
- **`insert` (all six)** — the injected literal is a **non-final column of a
  4-column `VALUES` tuple** (`VALUES (10,1,'{input}','inserted')`). Closing the
  tuple early (`')`) yields a 3-value tuple against a 4-column table → arity
  error; commenting forward drops `,'inserted')` → same. sqlmap's boundaries are
  generic and cannot reconstruct the trailing `,'inserted')`, so no technique
  produces a valid INSERT. (This is stronger than a level gate — even the level-4
  `')`+comment boundary hits the arity wall.)

### 7b. Reachable only above the pinned level (level-gated, not impossible)

These would pass at `--level 4/5` but are refused here by `extending=N`:

- **`insert` / `update-value` B,E,T** — need the pre-WHERE clause-9 boundaries
  (`' WHERE [RANDNUM]=[RANDNUM]`, level 4) or `')`+comment (level 4). (For
  `insert`, note the arity wall in 7a still bites even at level 4 — so `insert`
  is effectively 7a, not merely level-gated.)
- **leading-projected-identifier B** (`column`, `group`,
  `pipeline-projection`, `stored-identifier`) — only via the level-5 clause-8
  `"="[ORIGINAL]"` identifier boundary; U/S/T/Q remain 7a even there.

### 7c. Reachable at level 3 today, just not with the current control shape

- **Q** never (7a).
- The cleanest conventional remedies for the level-gated set, if the cases are to
  keep all six techniques without raising the profile, are shape changes to the
  *control only*: put the injected literal in the **final** VALUES column (or use
  `INSERT … SELECT … WHERE`) for `insert` S; move leading projected identifiers to
  a **trailing** identifier slot for `column`/`group`/etc. Each such change makes
  the control a different (still conventional) injection context, which is the
  honest trade-off to surface rather than work around.

### 7d. Double-quoted identifiers — the suffix that decides them

Filtering `boundaries.xml` to `<level> ≤ 3` leaves exactly **five** boundaries
that close a double quote, and every one appends a comparison between two
*invented* double-quoted identifiers:

```xml
<boundary><level>2</level><clause>1</clause><where>1,2</where><ptype>4</ptype>
  <prefix>"</prefix><suffix> AND "[RANDSTR]"="[RANDSTR]</suffix></boundary>
<boundary><level>2</level>…<ptype>4</ptype><prefix>")</prefix>  <suffix> AND ("[RANDSTR]"="[RANDSTR]</suffix></boundary>
<boundary><level>3</level>…<ptype>4</ptype><prefix>"))</prefix> <suffix> AND (("[RANDSTR]"="[RANDSTR]</suffix></boundary>
<boundary><level>3</level>…<ptype>5</ptype><prefix>"</prefix>   <suffix> AND "[RANDSTR]" LIKE "[RANDSTR]</suffix></boundary>
<boundary><level>3</level>…<ptype>5</ptype><prefix>")</prefix>  <suffix> AND ("[RANDSTR]" LIKE "[RANDSTR]</suffix></boundary>
```

The single closer whose suffix is a comment — `<ptype>4</ptype><prefix>"</prefix>
<suffix>[GENERIC_SQL_COMMENT]</suffix>` — is **level 5**, refused by
`--answers extending=N`.

`suffixQuery` (`agent.py`) substitutes the payload's own `<comment>` for the
boundary suffix whenever the test carries one. So escaping a double-quoted
identifier at this profile reduces to one question: does any qualifying test of
that technique carry a `<comment>`?

| tech | qualifying tests at L≤3/R≤2 carrying `<comment>` |
| --- | --- |
| B | 3 of 13 — two `[GENERIC_SQL_COMMENT]`, one `--` (stacked) |
| E | **none of 3** |
| U | 11 of 11 — `[GENERIC_SQL_COMMENT]` |
| S | 2 of 2 — `--` |
| T | **none of 4** |
| Q | none, and `where=3` regardless (§4) |

**E and T therefore cannot escape any double-quoted identifier here**, whatever
surrounds it: the appended `"[RANDSTR]"="[RANDSTR]"` names columns no relation
has, so PostgreSQL rejects the statement during name resolution — before the
error-forcing CAST is evaluated or the timing subquery is reached. That is a
property of the context class, not of a control's shape, and it is what
`enum-E`, `enum-ddl-E`, `enum-T` and `enum-ddl-T` run into.

B is not decided by the comment alone. With one it deletes whatever follows the
injection point; without one it inherits the invented identifiers. Two outcomes
matter:

- **Leading projected identifier** (`SELECT "{input}" FROM items`): the comment
  deletes the `FROM items` the projected name resolves against; the suffix
  leaves `SELECT (text AND bool)`, a type error; the stacked variant loses its
  `FROM` on the first statement. Only the level-5 clause-8 `"="[ORIGINAL]"`
  boundary rewrites the slot into `"c"="c" AND <inference>`, which does parse —
  so B here is refused by the level ceiling, and relocating the identifier to a
  trailing slot restores U and S but still not B, at the price of turning the
  case into the `table`/`order` context.
- **CREATE TABLE column definition** (`enum-ddl`): a column-definition list has
  no truth slot and no set-operation position, so neither an `AND <inference>`
  nor a `UNION SELECT` can attach however the quote is closed. Only
  `;<statement>` attaches, and the statement it appends returns its own result
  set rather than changing the original one — the difference B measures. This is
  why the DDL context caps at S.

**What a truth slot changes.** The same closed identifier is reachable once the
surrounding statement puts it in a WHERE. Filtering an enum-typed column on the
injected type name —

```sql
SELECT name FROM reviews WHERE status = CAST('ready' AS fixture."{input}")
```

— leaves `") AND <inference>-- ` parsing cleanly, and `") UNION ALL SELECT
<marker>-- ` projecting a text column a marker can ride; B and U both fire, S is
unaffected, E and T remain blocked for the reason above. A *projected* cast
(`SELECT CAST('ready' AS fixture."{input}")::text`) reaches neither: the closed
identifier sits directly in front of an enum-typed value, where `AND` is a type
error and every UNION marker sqlmap can place — `[CHAR]`, `[RANDNUM]`, a quoted
literal — is refused against the enum, leaving only `NULL`, which reflects
nothing. The difference is the truth slot, not the identifier.

---

## 8. Confidence

- **Q is 0/35 and structurally inapplicable (§4):** high. Rests on two
  independently verified facts — the sole `where=3` boundary is the empty/empty
  `clause 0` entry (grep of `boundaries.xml`), and the Q path is GREP-on-response
  (`checks.py`). Both are direct reads, and the mechanism explains all 35 misses.
- **U struct on DML and post-terminal clauses (§7a):** high. Pure SQL grammar; the
  acceptance table agrees on every such cell.
- **Leading-projected-identifier and `schema`/`function`/`insert` structural
  misses (§7a):** high for the "all six miss" prediction (matches acceptance
  exactly); high for the mechanism.
- **Level-3 boundary catalogue and the level-4/5 exclusions via `extending=N`
  (§2):** high — direct read of `boundaries.xml` levels plus the `checks.py`
  ceiling and the harness `--answers` string.
- **Bare-name tails capping at "U,S" / "S" (§7a) and the SET/VALUES→L≥4 gate
  (§7b):** medium-high. Boundary/level reasoning is solid and matches the
  observed pass sets; I did not re-execute sqlmap to watch each forged payload.
- **Exact mechanism of individual *passes* that involve type coercion (e.g.
  `cast` passing B, `limit`/`offset` passing B/E/T):** medium. The pass/fail
  outcomes are ground truth from the acceptance run and the *failing* cells
  (U, Q) are explained with high confidence; the precise boundary that carries
  each of those particular passes was not traced end-to-end.
