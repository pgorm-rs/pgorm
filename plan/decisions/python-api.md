---
id [dec:pgorm:python-api]
epitome "Provide an optional public PyO3 package that composes real pgorm operations at runtime, with explicit compiled registrations for Rust entity and graph types."
state @decided
category @existence
scope {
    elements ([arch:pgorm:python-api])
    rules (
        [spec:pgorm:def:python.api]
        [spec:pgorm:req:python.optional]
        [spec:pgorm:req:python.delegation]
        [spec:pgorm:req:python.capabilities]
        [spec:pgorm:req:python.models]
        [spec:pgorm:req:python.entities]
        [spec:pgorm:req:python.runtime]
        [spec:pgorm:req:python.codegen]
        [spec:pgorm:req:python.distribution]
    )
}
author "brendan@necessary.nu"
decided_at "2026-09-08T15:23:36Z"
establishes ([arch:pgorm:python-api])
alternatives (
    {
        option "Compile a new Rust executable for every generated query."
        rejected_because "Compilation scales with the number of cases even when only runtime values and builder composition change; it also produces no reusable Python package."
    }
    {
        option "Keep only fixed HTTP or stdin query routes."
        rejected_because "Changing transport leaves the query programs handwritten and does not expose a composable API to Python applications or generators."
    }
    {
        option "Implement SQL construction and escaping again in Python."
        rejected_because "The generator would exercise a second implementation, reducing both fidelity to pgorm and maintenance value."
    }
    {
        option "Expose only the repository's compiled fixture entities."
        rejected_because "That is useful test plumbing but cannot serve Python applications with their own schemas."
    }
)
consequences {
    accepted (
        "pgorm-python is an opt-in companion crate and distributable Python package; default Rust builds remain independent of Python tooling."
        "Runtime schema/model descriptors serve application schemas through existing Rust statement APIs; concrete Rust entities, ActiveModels and graph tuples use explicit precompiled registrations. The capability manifest distinguishes these execution paths."
        "Public bindings cover construction, tagged values, results, asyncio execution, TLS/pools, transactions, cancellation and streaming, with Python typing and package artifacts as deliverables."
        "Python wrappers own their native state and validate dynamic boundaries; the Rust type system and scoped binder lifetimes remain intact. No unchecked lifetime extension or silent generic-path substitution is permitted."
        "Downstream codegen can compile registrations for additional entity definitions and graph shapes once, then reuse them across queries. A Python schema declaration alone does not instantiate a Rust trait or run a derive macro."
        "A persistent Tokio runtime and pools amortize setup; the security campaign uses the public conversion and dispatch surface."
    )
    deferred (
        "Registry distribution-name availability, exact dependency pins, ABI choice and supported platform/interpreter matrix are packaging deliverables, not claims made by this decision."
        "A complete one-to-one Python spelling for every Rust trait, arbitrary runtime generic instantiation, a synchronous API and untested interpreter/free-threading combinations are outside initial acceptance."
    )
}
edges {
    requires ([dec:pgorm:no-panic] [dec:pgorm:invalid-states-unrepresentable])
    enables ([dec:pgorm:generated-orm-testing])
}
---

## Context and decision

The operator proposed PyO3 to avoid compiling a separate Rust program for every
generated test, then explicitly requested a useful public Python package as
well as the testing interface. The chosen boundary is a composable native
library: Python supplies operation structure and inputs; Rust owns pgorm query
construction and execution. These are planned components, not shipped bindings.

The public package must work with application tables outside this repository.
Runtime descriptors therefore lower to pgorm's dynamic statement builders and
record decoding. They do not claim the semantics of a concrete Rust entity's
hooks or typed graph projection. Those paths require a compiled registration,
which downstream users can build through the documented codegen workflow.

Rust rejects invalid typed states at compilation. Python cannot inherit all
those proofs across a dynamic FFI, so concrete wrappers preserve them where
possible and reject invalid ownership, types or scopes at the boundary where
necessary. This follows the existing invalid-state and no-panic decisions;
it does not loosen the underlying Rust APIs for convenience.

## Consequences for verification

The native extension compiles per source/features/ABI/registration set, not per
query. Binding parity and conversion tests are essential because the bridge
itself becomes part of the system under test. Generated Rust tests still cover
derives and generic shapes that a prebuilt extension cannot instantiate.

The `python` WBS and `docs/spec/python.md` carry package acceptance independently
of a full security campaign. Publication is a later release action; building
reviewable wheel and source artifacts is part of the planned work.

## Sources

- [PyO3 class restrictions](https://pyo3.rs/main/class#restrictions) describe
  lifetime ownership and concrete generic wrappers.
- [Maturin](https://www.maturin.rs/) supplies the native Python packaging workflow.
