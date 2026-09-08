---
id [dec:pgorm:generated-orm-testing]
epitome "Generate typed pgorm operation programs through the public Python bindings, reuse one native build, and retain independent checks plus minimized Python and Rust reproducers."
state @decided
category @executive
scope {
    elements ([arch:pgorm:generative-testing])
    rules (
        [spec:pgorm:def:generative.program]
        [spec:pgorm:req:generative.grammar]
        [spec:pgorm:req:generative.matrix]
        [spec:pgorm:req:generative.execution]
        [spec:pgorm:req:generative.build-amortization]
        [spec:pgorm:req:generative.oracles]
        [spec:pgorm:req:generative.replay]
        [spec:pgorm:req:generative.compile-suite]
    )
}
author "brendan@necessary.nu"
decided_at "2026-09-08T15:23:36Z"
establishes ([arch:pgorm:generative-testing])
alternatives (
    {
        option "Treat an external sqlmap scan of fixed routes as generated ORM-program coverage."
        rejected_because "sqlmap supplies contextual SQL attack strings and detector logic; it does not generate Rust builder compositions, entity definitions or operation sequences."
    }
    {
        option "Translate each SQL attack fragment into intentional pgorm boolean or raw-SQL operations."
        rejected_because "Turning a hostile string into an intentional OR changes the test semantics. The generator must independently choose API structure while the attack remains data in its declared input context."
    }
    {
        option "Judge correctness by parser success, absence of exceptions or agreement with the subject's own SQL."
        rejected_because "These checks can all accept a query that returns or mutates the wrong rows. Independent observations and positive controls are required."
    }
    {
        option "Use only PyO3 runtime tests for all Rust guarantees."
        rejected_because "Precompiled bindings cannot explore fresh derives, lifetime failures or generic instantiations; a separate bounded compile suite is still needed."
    }
)
consequences {
    accepted (
        "A portable typed operation graph is the common input to runtime execution, structure-aware shrinking and Rust source emission. It is a test artifact, not a second public ORM."
        "Python generates compositions and effect sequences through public bindings; test-only concrete entity/graph registrations preserve paths unavailable through runtime descriptors."
        "An independent reference model/query or a justified metamorphic/state invariant checks each executed case; controls prove that the check can detect its intended defect."
        "Programs run in disposable restricted PostgreSQL fixtures with fresh baselines for generation, shrinking and replay; state is preserved between operations within one program."
        "Failures retain exact program/fixture/build identities and Python replay. Rust emission uses actual pgorm operations and supports attribution when only the Python boundary fails."
        "The existing sqlmap branch retains its external-scanner contract. Its pinned corpus is an optional generator input, not a dependency on successful HTTP scans or equivalent scanner evidence."
        "Runtime coverage, compile coverage, binding parity and external-scanner evidence have separate verdicts. Cold compilation and native runtime throughput are measured separately."
    )
    deferred (
        "A specific generator engine and exact dependency versions are implementation choices; Hypothesis is a candidate, while portable replay cannot depend on its internal example format."
        "Exhaustive SQL/ORM correctness proofs and generation outside the declared capability/coverage matrix are not acceptance claims."
    )
}
edges {
    requires ([dec:pgorm:python-api])
}
---

## Context and decision

The requested unit of variation is a pgorm program. A different HTTP transport
would not change that unit: the existing protected handlers still select a
handwritten builder chain. Generating operations through compiled PyO3 bindings
lets the structure and inputs vary without a compiler invocation for each case.

The grammar tracks available schema types, query sources, binders, transactions
and earlier results. It generates both single queries and stateful sequences,
including stored input reused as a value or identifier. A manifest makes the
actual Rust API and registered graph shapes visible, so dynamic statement
coverage cannot conceal an untested typed ORM path.

SQL injection is one failure class alongside incorrect grouping, casts,
parameter ordering, decode shapes, transaction effects and resource lifetime.
The oracle must observe those semantics independently of the implementation
being checked. A successful SQL parse or a quiet scanner cannot establish them.

## Relationship to existing work

The `generative` WBS depends on the necessary `python` leaves and existing Rust
capabilities. It neither gates the basic Python package on an external scan nor
waits for the entire `sqlmap` branch. Existing scanner fixtures and pinned
payload data can be reused after checking their suitability; successful scanner
controls remain evidence only for that scanner campaign.

The new specs add requirements rather than redefining the HTTP scanner rules.
This preserves the meaning of existing annotations and completed `sqlmap.spec`
work. A prose scope note in that chapter points to the broader program campaign.

## Sources

- [Hypothesis stateful testing](https://hypothesis.readthedocs.io/en/latest/stateful.html)
  illustrates generated action sequences and reusable prior results.
- [sqlmap techniques](https://github.com/sqlmapproject/sqlmap/wiki/Techniques)
  describe attack generation and response-based detection, not pgorm generation.
