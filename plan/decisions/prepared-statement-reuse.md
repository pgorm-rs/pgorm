---
id [dec:pgorm:prepared-statement-reuse]
epitome "Reuse prepared statements through the per-connection cache on the ordinary execution path; do not introduce a typed statement handle that pins a connection."
state @tentative
category @executive
scope {
    rules ([spec:pgorm:sem:conn.pool.statement-cache] [spec:pgorm:def:conn.pool.conn-trait] [spec:pgorm:def:exec.decode])
}
author "brendan@necessary.nu"
alternatives (
    {
        option "A first-class typed handle, conn.prepare::<M>(sql), holding the tokio_postgres::Statement and its Describe metadata."
        rejected_because "Statements are per-connection, so the handle must pin a pooled connection — measured pool starvation under a two-slot pool — and it buys only ~10% over cache routing (2.36x vs 1.93x at zero latency). It also widens the existing hole where ConnectionTrait accepts a Statement from a foreign connection and fails at runtime with SQLSTATE 26000."
    }
    {
        option "A typed handle carrying only SQL text and expected types, re-resolved through the cache per checkout."
        rejected_because "Coherent — re-resolution costs 32ns and no wire traffic — but once ConnectionTrait routes through the cache, the handle's only remaining content is the type check, which is better offered as a verification utility than as a type callers must thread through their code."
    }
    {
        option "Leave it: the statement cache suffices."
        rejected_because "It does not run. No pgorm code path calls prepare_cached, and DatabaseConnection's pooled object is crate-private with no accessor, so no external caller can reach it either. Measured on the wire, every ORM query today pays Parse + Describe + Close on top of Bind + Execute: three syncs where one would do."
    }
)
consequences {
    accepted (
        "ConnectionTrait's six extended-protocol methods resolve SqlText::sql_text() through the connection's StatementCache; since infra.statement-identity sealed the bound to str/String there is no second route. Measured 2.30x on real ORM queries at a 4ms round trip."
        "Reuse introduces one failure mode the re-prepare-every-time path lacks: SQLSTATE 0A000, cached plan must not change result type, after DDL that alters a result column. The cache evicts and re-prepares once; a second failure surfaces to the caller."
        "The cache key space is unbounded — variable-arity IN lists alone produced 25 entries for one logical query — so the cache carries a capacity bound and an opt-out."
        "Prepare-time verification of a FromQueryResult target against Statement::columns() is offered as a utility, not as a handle type. It is the only thing that catches a wrong target when the query returns no rows, which today returns Ok(vec![]) and ships."
    )
    deferred ("A typed handle remains open for the sql! macro, where libpg_query already validates the text at compile time and could carry an inferred row shape; that is the case where the handle earns the ceremony. The cross-connection Statement hole (SQLSTATE 26000) was closed by infra.statement-identity: ConnectionTrait no longer accepts a Statement at all.")
}
edges {
    related_to ([dec:pgorm:invalid-states-unrepresentable])
}
---

## Rationale

The question was whether a typed prepared-statement surface earns its
keep. Measuring the machinery first answered a different and more
urgent one: pgorm prepares every statement afresh on every call and
throws it away. The per-connection cache that would prevent this
exists, is documented, and is called by nothing — not by pgorm, and
not reachably by any user, since the pooled object is crate-private.

On the wire that costs three round trips per query where one suffices:
Parse and Describe, then Bind and Execute, then a Close on drop. Cache
misses are linear in pool size, not in query count — a sixteen-slot
pool paid sixteen Parses for forty executions of the same statement —
so the "different connections, different caches" objection prices out
as a fixed startup cost, not an ongoing one.

Against that, a typed handle is a small increment bought at a
disproportionate price. Pinning a connection for a handle's lifetime
fights the pool for a tenth of the win the cache already gives, and
the alternative — a handle that re-resolves per checkout — reduces to
a type assertion wearing a type's clothes. The genuine gap the
Describe metadata closes is narrower and sharper than a handle: a
FromQueryResult target with a wrong column type, or naming a column
that does not exist, decodes successfully against zero rows. That
failure is invisible until data arrives. Statement::columns() sees it
at prepare time whether rows exist or not, and that is worth a
verification call — not a type that has to be threaded through every
call site to be useful.
