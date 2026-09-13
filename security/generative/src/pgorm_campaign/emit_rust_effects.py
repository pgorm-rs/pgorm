"""Effect steps and transaction scopes over the named pgorm terminals."""

from .emit_rust_models import ACCOUNT, TYPED_ROWS
from .emit_rust_values import (
    PL,
    Q,
    REPLAY,
    UnsupportedInstruction,
    boolean,
    literal,
)

ISOLATION = {
    "read_committed": "ReadCommitted",
    "repeatable_read": "RepeatableRead",
    "serializable": "Serializable",
}
# Which pgorm terminal the Python binding selects for a fetch, by query type.
FETCH_PREFIX = {
    "entity_query": "pgorm::Select",
    "graph_query": "pgorm::SelectGraph",
    "cursor": "pgorm::Cursor",
    "pipeline": "pgorm::pipeline::Pipeline",
    "sources": "pgorm::pipeline::SelectedSources",
}
# Statement shapes a runtime model descriptor lowers to, which are the ordinary
# builder statements and carry the ordinary `build`.
MODEL_QUERIES = ("model_query", "model_write")


# [spec:pgorm:req:generative.replay]
class EffectEmitter:
    """Emit one scheduled effect, recording what the step observed."""

    def compile_query(self, reference):
        """The `(sql, values)` pair an effect runs, built by the named builder."""
        kind = self.types[reference]
        query = self.use(reference)
        if kind in ("select", "insert", "update", "delete") + MODEL_QUERIES:
            return f"{query}.build()"
        if kind == "raw":
            return query
        if kind == "ddl":
            return f"({query}, {Q}::Values(Vec::new()))"
        if kind in ("pipeline", "sources"):
            self.helpers.add("compiled")
            return f"compiled({query}.into_sql())?"
        if kind in ("entity_query", "graph_query"):
            # `QueryTrait::build` is what the binding's `inspect` reaches for a
            # compiled query, and it takes the query by reference.
            return f"pgorm::QueryTrait::build(&{query})"
        if kind == "cursor":
            raise UnsupportedInstruction(
                "pgorm::Cursor exposes no statement builder, so it cannot be inspected"
            )
        raise UnsupportedInstruction("no standalone Rust terminal for a " + kind)

    def row_observation(self, reference):
        """One decoded row of a typed read, in the shape the oracle produces."""
        kind = self.types[reference]
        if kind == "entity_query":
            registration = self.registration(reference)
            entity = self.entity_type(registration)
            return (
                f"{REPLAY}::observe::model::<{entity}>(row, {literal(registration)})?"
            )
        if kind in ("graph_query", "cursor"):
            _, _, slots = self.graph_shape(reference)
            root = self.entity_type(ACCOUNT)
            if not slots:
                # A root-only graph decodes to the model itself, and the binding
                # reports it as the record rather than as a one-slot tuple.
                return f"{REPLAY}::observe::model::<{root}>(row, {literal(ACCOUNT)})?"
            items = [f"{REPLAY}::observe::model::<{root}>(&row.0, {literal(ACCOUNT)})?"]
            for index, (registration, required) in enumerate(slots, 1):
                entity = self.entity_type(registration)
                name = literal(registration)
                items.append(
                    f"{REPLAY}::observe::model::<{entity}>(&row.{index}, {name})?"
                    if required
                    else f"{REPLAY}::observe::maybe::<{entity}>"
                    f"(row.{index}.as_ref(), {name})?"
                )
            return f"{REPLAY}::observe::tuple(vec![{', '.join(items)}])"
        if kind == "sources":
            _, _, entities = self.source_shape(reference)
            items = []
            for index, registration in enumerate(entities):
                entity = self.entity_type(registration)
                # A single listed source decodes to a bare `Option<Model>`, but
                # it is still reported as a tuple: an absent one-source row is
                # `(None,)`, which is not the same as no row at all.
                slot = "row" if len(entities) == 1 else f"row.{index}"
                items.append(
                    f"{REPLAY}::observe::maybe::<{entity}>"
                    f"({slot}.as_ref(), {literal(registration)})?"
                )
            return f"{REPLAY}::observe::tuple(vec![{', '.join(items)}])"
        raise UnsupportedInstruction("no compiled row observation for a " + kind)

    def typed_fetch(self, step, pad, connection):
        """Read a terminal that decodes compiled models rather than raw rows."""
        d, i = step["data"], step["inputs"]
        reference, mode = i["query"], d["mode"]
        kind = self.types[reference]
        query = self.use(reference)
        lines = []
        if kind == "cursor":
            if mode != "all":
                raise UnsupportedInstruction(
                    "pgorm::Cursor has no public " + mode + " terminal"
                )
            lines.append(f"{pad}    let mut cursor = {query};")
            lines.append(f"{pad}    let rows = cursor.all(&{connection}).await?;")
        else:
            if kind == "graph_query" and mode == "one":
                raise UnsupportedInstruction(
                    "pgorm::SelectGraph has no public one terminal"
                )
            terminal = "one_opt" if mode == "optional" else mode
            call = f"{query}.{terminal}(&{connection}).await?"
            if mode == "all":
                lines.append(f"{pad}    let rows = {call};")
            elif mode == "one":
                lines.append(f"{pad}    let rows = vec![{call}];")
            else:
                lines.append(
                    f"{pad}    let rows = {call}.into_iter().collect::<Vec<_>>();"
                )
        lines.append(f"{pad}    let mut observed = Vec::with_capacity(rows.len());")
        lines.append(f"{pad}    for row in &rows {{")
        lines.append(f"{pad}        observed.push({self.row_observation(reference)});")
        lines.append(f"{pad}    }}")
        lines.append(
            f"{pad}    let observation = {REPLAY}::observe::collected(observed);"
        )
        return lines

    def fetch_paths(self, reference, mode):
        kind = self.types[reference]
        if kind == "pipeline":
            return [f"{PL}::Pipeline::into_sql", "pgorm::ConnectionTrait::query_raw"]
        if kind in FETCH_PREFIX:
            method = "one_opt" if mode == "optional" else mode
            return [FETCH_PREFIX[kind] + "::" + method]
        return ["pgorm::ConnectionTrait::query_raw"]

    def effect(self, step, lines, indent):
        name, d, i = step["op"], step["data"], step["inputs"]
        connection = self.connection(step["scope"])
        pad = indent
        if name == "fetch":
            keep = step["id"] in self.results
            if self.types[i["query"]] in TYPED_ROWS:
                body = self.typed_fetch(step, pad, connection)
            else:
                method = {
                    "all": "query_all",
                    "one": "query_one",
                    "optional": "query_opt",
                }[d["mode"]]
                wrap = {
                    "all": None,
                    "one": "vec![rows]",
                    "optional": "rows.into_iter().collect::<Vec<_>>()",
                }[d["mode"]]
                self.helpers.add("params")
                body = [
                    f"{pad}    let (sql, values) = {self.compile_query(i['query'])};",
                    f"{pad}    let held = holders(&values);",
                    f"{pad}    let rows = pgorm::ConnectionTrait::{method}("
                    f"&{connection}, &sql, &params(&held)).await?;",
                ]
                if wrap is not None:
                    body.append(f"{pad}    let rows = {wrap};")
                body.append(
                    f"{pad}    let observation = {REPLAY}::observe::rows(&rows)?;"
                )
            body.append(
                f"{pad}    Ok((observation, rows))"
                if keep
                else f"{pad}    Ok(observation)"
            )
            self.record(step, body, lines, pad, rows=keep)
        elif name == "stream":
            self.helpers.add("params")
            keep = step["id"] in self.results
            body = [
                f"{pad}    let (sql, values) = {self.compile_query(i['query'])};",
                f"{pad}    let held = holders(&values);",
                f"{pad}    let opened = pgorm::ConnectionTrait::query_raw("
                f"&{connection}, &sql, params(&held)).await?;",
                f"{pad}    let drained = {REPLAY}::stream::drain(opened, "
                f"{int(d['take'])}usize, {boolean(d['cancel'])}).await?;",
                f"{pad}    let observation = {REPLAY}::observe::stream("
                "&drained.rows, drained.complete, drained.cancelled, drained.closed)?;",
                f"{pad}    let rows = drained.rows;",
            ]
            body.append(
                f"{pad}    Ok((observation, rows))"
                if keep
                else f"{pad}    Ok(observation)"
            )
            self.record(step, body, lines, pad, rows=keep)
        elif name == "active.write":
            registration = self.registration(i["model"])
            model = self.use(i["model"])
            if d["method"] == "delete":
                body = [
                    f"{pad}    let count = pgorm::ActiveModelTrait::delete("
                    f"{model}, &{connection}).await?;",
                    f"{pad}    Ok({REPLAY}::observe::count(count))",
                ]
                self.record(step, body, lines, pad)
            else:
                keep = step["id"] in self.results
                entity = self.entity_type(registration)
                body = [
                    f"{pad}    let written = pgorm::ActiveModelTrait::{d['method']}("
                    f"{model}, &{connection}).await?;",
                    f"{pad}    let observation = {REPLAY}::observe::collected(vec![",
                    f"{pad}        {REPLAY}::observe::model::<{entity}>("
                    f"&written, {literal(registration)})?,",
                    f"{pad}    ]);",
                ]
                body.append(
                    f"{pad}    Ok((observation, vec![written]))"
                    if keep
                    else f"{pad}    Ok(observation)"
                )
                self.record(step, body, lines, pad, rows=keep)
        elif name == "execute":
            self.helpers.add("params")
            body = [
                f"{pad}    let (sql, values) = {self.compile_query(i['query'])};",
                f"{pad}    let held = holders(&values);",
                f"{pad}    let count = pgorm::ConnectionTrait::execute("
                f"&{connection}, &sql, &params(&held)).await?;",
                f"{pad}    Ok({REPLAY}::observe::count(count))",
            ]
            self.record(step, body, lines, pad)
        elif name == "inspect":
            self.helpers.add("tagged")
            body = [
                f"{pad}    let (sql, values) = {self.compile_query(i['query'])};",
                f"{pad}    Ok({REPLAY}::observe::compiled(&sql, &tagged(&values))?)",
            ]
            self.record(step, body, lines, pad)
        elif name in ("commit", "rollback"):
            body = [
                f"{pad}    pgorm::DatabaseTransaction::{name}({connection}).await?;",
                f"{pad}    Ok({REPLAY}::observe::unit())",
            ]
            self.record(step, body, lines, pad)
        else:
            raise UnsupportedInstruction("no standalone Rust source for " + name)

    def record(self, step, body, lines, pad, *, rows=False):
        rendered = ", ".join(literal(path) for path in self.paths(step))
        lines.append(f"{pad}let outcome: Result<_, Error> = async {{")
        lines.extend(body)
        lines.append(f"{pad}}}")
        lines.append(f"{pad}.await;")
        identity = f"{literal(step['id'])}, {literal(step['op'])}"
        # Rows are kept only where a later result reference reads them back.
        binding = f"{pad}let r_{step['id']} = " if rows else f"{pad}"
        lines.append(binding + "match outcome {")
        if rows:
            lines.append(
                f"{pad}    Ok((observation, rows)) => {{ report.observed("
                f"{identity}, &[{rendered}], observation); rows }}"
            )
            lines.append(
                f"{pad}    Err(error) => {{ report.failed({identity}, &[], "
                "&harness.observe(&error)); Vec::new() }"
            )
            lines.append(f"{pad}}};")
        else:
            lines.append(
                f"{pad}    Ok(observation) => report.observed({identity}, "
                f"&[{rendered}], observation),"
            )
            lines.append(
                f"{pad}    Err(error) => report.failed({identity}, &[], "
                "&harness.observe(&error)),"
            )
            lines.append(f"{pad}}}")

    def paths(self, step):
        name, d, i = step["op"], step["data"], step["inputs"]
        if name == "fetch":
            return self.fetch_paths(i["query"], d["mode"])
        if name == "execute":
            return ["pgorm::ConnectionTrait::execute_raw"]
        if name == "stream":
            return ["pgorm::ConnectionTrait::query_raw", "tokio_postgres::RowStream"]
        if name == "active.write":
            return ["pgorm::ActiveModelTrait::" + d["method"]]
        if name == "inspect":
            if self.types[i["query"]] in ("pipeline", "sources"):
                return [f"{PL}::Pipeline::into_sql"]
            return ["pgorm_query::QueryStatementBuilder"]
        if name == "begin":
            return [
                "pgorm::TransactionTrait::begin_with"
                if step["scope"] == "root"
                else "pgorm::TransactionTrait::begin"
            ]
        if name in ("commit", "rollback"):
            return ["pgorm::DatabaseTransaction::" + name]
        raise UnsupportedInstruction("no standalone Rust source for " + name)

    def transaction_mode(self, data):
        mode, isolation = data["mode"], data["isolation"]
        if mode == "default":
            if isolation != "default":
                raise UnsupportedInstruction(
                    "a default transaction mode carries no isolation level"
                )
            return "pgorm::TransactionMode::Default"
        level = (
            "None"
            if isolation == "default"
            else f"Some(pgorm::IsolationLevel::{ISOLATION[isolation]})"
        )
        variant = "ReadWrite" if mode == "read_write" else "ReadOnly"
        return f"pgorm::TransactionMode::{variant} {{ isolation: {level} }}"

    def begin(self, step, lines, indent, remaining):
        pad = indent
        child = self.connection(step["data"]["child"])
        identity = f"{literal(step['id'])}, {literal(step['op'])}"
        paths = ", ".join(literal(path) for path in self.paths(step))
        if step["scope"] == "root":
            mode = self.transaction_mode(step["data"])
            opened = (
                f"pgorm::DatabaseConnection::begin_with(&mut "
                f"{self.connection(step['scope'])}, {mode}).await"
            )
        else:
            opened = (
                f"pgorm::TransactionTrait::begin(&mut "
                f"{self.connection(step['scope'])}).await"
            )
        lines.append(f"{pad}match {opened} {{")
        lines.append(
            f"{pad}    Err(error) => report.failed({identity}, &[], "
            "&harness.observe(&Error::from(error))),"
        )
        lines.append(f"{pad}    Ok(mut {child}) => {{")
        scope_name = literal(step["data"]["child"])
        lines.append(
            f"{pad}        report.observed({identity}, &[{paths}], "
            f"{REPLAY}::observe::transaction({scope_name}));"
        )
        consumed = self.steps(remaining, lines, pad + "        ")
        lines.append(f"{pad}    }}")
        lines.append(f"{pad}}}")
        return consumed

    def steps(self, remaining, lines, indent):
        """Emit steps until the scope opened by the caller closes."""
        consumed = 0
        while remaining:
            step = remaining[0]
            consumed += 1
            remaining = remaining[1:]
            if step["op"] == "begin":
                nested = self.begin(step, lines, indent, remaining)
                consumed += nested
                remaining = remaining[nested:]
                continue
            outer, self.current = self.current, set()
            body = []
            self.effect(step, body, indent)
            used, self.current = self.current, outer | self.current
            self.declare(used, lines, indent)
            lines.extend(body)
            if step["op"] in ("commit", "rollback"):
                break
        return consumed
