"""Borrows and brands: the contracts whose whole content is a compile refusal.

`DatabaseTransaction<'a>` borrows the connection it began on, and `Binder`
brands the expressions it mints with a lifetime the pipeline invents per call.
Neither contract can be observed at runtime: a program that violates one never
exists as a binary, and a program that respects one is indistinguishable from a
program that had no such contract. The evidence is the rejection itself.
"""

from .compile_case import CompileCase, rejects
from .compile_entities import entity_module

OBLIGATION = "rust-ownership"

CONN_PRELUDE = (
    "    use pgorm::{ConnectionTrait, DatabaseConnection, DatabasePool, "
    "DatabaseTransaction, Error, TransactionTrait};"
)

PIPELINE_ENTITIES = "\n\n".join(
    [
        "    pub mod left {\n"
        + "\n".join(
            "    " + line if line else line
            for line in entity_module(
                table="brand_left", columns=(("label", "String", ""),)
            ).splitlines()
        )
        + "\n    }",
        "    pub mod right {\n"
        + "\n".join(
            "    " + line if line else line
            for line in entity_module(
                table="brand_right", columns=(("weight", "i64", ""),)
            ).splitlines()
        )
        + "\n    }",
    ]
)


def _transaction(identity, body, *, verdict, phase, expects=None, note=""):
    return CompileCase(
        id=identity,
        obligation=OBLIGATION,
        verdict=verdict,
        phase=phase,
        source=CONN_PRELUDE + "\n\n" + body,
        expects=expects,
        note=note,
    )


def transaction_accept_cases():
    """The uses the borrow permits, so a rejection cannot be read as blanket."""
    commit = (
        "    pub async fn run(conn: &mut DatabaseConnection) -> Result<(), Error> {\n"
        "        let tx = conn.begin().await?;\n"
        "        tx.commit().await?;\n"
        "        Ok(())\n    }"
    )
    sequential = (
        "    pub async fn run(conn: &mut DatabaseConnection) -> Result<(), Error> {\n"
        "        let first = conn.begin().await?;\n"
        "        first.rollback().await?;\n"
        "        let second = conn.begin().await?;\n"
        "        second.commit().await?;\n"
        "        Ok(())\n    }"
    )
    savepoint = (
        "    pub async fn run(conn: &mut DatabaseConnection) -> Result<(), Error> {\n"
        "        let mut tx = conn.begin().await?;\n"
        "        let inner = tx.begin().await?;\n"
        "        inner.commit().await?;\n"
        "        tx.commit().await?;\n"
        "        Ok(())\n    }"
    )
    reborrow = (
        "    pub async fn run(conn: &mut DatabaseConnection) -> Result<(), Error> {\n"
        "        let tx = conn.begin().await?;\n"
        '        tx.batch_execute("SELECT 1").await?;\n'
        "        tx.commit().await?;\n"
        '        conn.batch_execute("SELECT 2").await?;\n'
        "        Ok(())\n    }"
    )
    return [
        _transaction(
            "txn-commit",
            commit,
            verdict="accept",
            phase="borrowck",
            note="one transaction, consumed by commit",
        ),
        _transaction(
            "txn-sequential",
            sequential,
            verdict="accept",
            phase="borrowck",
            note="the borrow ends when the handle is consumed",
        ),
        _transaction(
            "txn-savepoint",
            savepoint,
            verdict="accept",
            phase="borrowck",
            note="a nested savepoint borrows the transaction, not the connection",
        ),
        _transaction(
            "txn-after-commit",
            reborrow,
            verdict="accept",
            phase="borrowck",
            note="the connection is usable again once the transaction is gone",
        ),
    ]


def transaction_reject_cases():
    """Each way the borrow can be broken, and the code that names the break."""
    variants = (
        (
            "txn-commit-twice",
            "    pub async fn run(conn: &mut DatabaseConnection) -> Result<(), Error> {\n"
            "        let tx = conn.begin().await?;\n"
            "        tx.commit().await?;\n"
            "        tx.rollback().await?;\n"
            "        Ok(())\n    }",
            rejects("E0382"),
            "commit consumes the handle, so there is no second outcome",
        ),
        (
            "txn-conn-while-open",
            "    pub async fn run(conn: &mut DatabaseConnection) -> Result<(), Error> {\n"
            "        let tx = conn.begin().await?;\n"
            '        conn.batch_execute("SELECT 1").await?;\n'
            "        tx.commit().await?;\n"
            "        Ok(())\n    }",
            rejects("E0502"),
            "the open transaction holds the connection exclusively",
        ),
        (
            "txn-begin-twice",
            "    pub async fn run(conn: &mut DatabaseConnection) -> Result<(), Error> {\n"
            "        let first = conn.begin().await?;\n"
            "        let second = conn.begin().await?;\n"
            "        second.commit().await?;\n"
            "        first.commit().await?;\n"
            "        Ok(())\n    }",
            rejects("E0499"),
            "two live transactions on one connection is two mutable borrows",
        ),
        (
            "txn-escapes-connection",
            "    pub async fn run(pool: &DatabasePool)\n"
            "        -> Result<DatabaseTransaction<'static>, Error> {\n"
            "        let mut conn = pool.get().await?;\n"
            "        let tx = conn.begin().await?;\n"
            "        Ok(tx)\n    }",
            rejects("E0515"),
            "a transaction cannot outlive the connection it began on",
        ),
        (
            "txn-stored-past-connection",
            "    pub async fn run(pool: &DatabasePool) -> Result<(), Error> {\n"
            "        let mut held: Option<DatabaseTransaction<'_>> = None;\n"
            "        {\n"
            "            let mut conn = pool.get().await?;\n"
            "            held = Some(conn.begin().await?);\n"
            "        }\n"
            "        if let Some(tx) = held {\n"
            "            tx.commit().await?;\n"
            "        }\n"
            "        Ok(())\n    }",
            rejects("E0597", "E0505"),
            "storing the handle does not extend the connection's life",
        ),
    )
    return [
        _transaction(
            identity,
            body,
            verdict="reject",
            phase="borrowck",
            expects=expectation,
            note=note,
        )
        for identity, body, expectation, note in variants
    ]


def _pipeline(identity, body, *, verdict, phase, expects=None, note=""):
    return CompileCase(
        id=identity,
        obligation=OBLIGATION,
        verdict=verdict,
        phase=phase,
        source=PIPELINE_ENTITIES + "\n\n" + body,
        expects=expects,
        note=note,
    )


def binder_accept_cases():
    """Two pipelines, each binding its own values, which is the legal shape."""
    body = (
        "    use pgorm::pipeline::{ExprOps, Pipeline};\n\n"
        "    pub fn build() {\n"
        "        let _left = Pipeline::from(left::Entity)\n"
        "            .filter_with(|binder| left::Column::Id.eq(binder.bind(1_i32)));\n"
        "        let _right = Pipeline::from(right::Entity)\n"
        "            .filter_with(|binder| right::Column::Id.eq(binder.bind(2_i32)));\n"
        "    }"
    )
    nested = (
        "    use pgorm::pipeline::{ExprOps, Pipeline};\n\n"
        "    pub fn build() {\n"
        "        let _outer = Pipeline::from(left::Entity).filter_with(|binder| {\n"
        "            let first = binder.bind(1_i32);\n"
        "            let second = binder.bind(2_i32);\n"
        "            left::Column::Id.eq(first).and(left::Column::Id.ne(second))\n"
        "        });\n"
        "    }"
    )
    return [
        _pipeline(
            "binder-separate-pipelines",
            body,
            verdict="accept",
            phase="borrowck",
            note="each pipeline binds through its own binder",
        ),
        _pipeline(
            "binder-two-values",
            nested,
            verdict="accept",
            phase="borrowck",
            note="one brand, several values, composed inside the closure",
        ),
    ]


def binder_reject_cases():
    """Mixing brands across pipelines, at the escape and at the use."""
    escape = (
        "    use pgorm::pipeline::{Expr, ExprOps, Pipeline};\n\n"
        "    pub fn build() {\n"
        "        let mut smuggled: Option<Expr<'static>> = None;\n"
        "        let _left = Pipeline::from(left::Entity).filter_with(|binder| {\n"
        "            let bound = binder.bind(1_i32);\n"
        "            smuggled = Some(bound.clone());\n"
        "            left::Column::Id.eq(bound)\n"
        "        });\n"
        "        let _right = Pipeline::from(right::Entity)\n"
        "            .filter_with(move |_| right::Column::Id.eq(smuggled.clone().unwrap()));\n"
        "    }"
    )
    nested = (
        "    use pgorm::pipeline::{ExprOps, Pipeline};\n\n"
        "    pub fn build() {\n"
        "        let _outer = Pipeline::from(left::Entity).filter_with(|outer| {\n"
        "            let _inner = Pipeline::from(right::Entity)\n"
        "                .filter_with(|_| right::Column::Id.eq(outer.bind(1_i32)));\n"
        "            left::Column::Id.eq(outer.bind(2_i32))\n"
        "        });\n"
        "    }"
    )
    return [
        _pipeline(
            "binder-brand-escapes",
            escape,
            verdict="reject",
            phase="borrowck",
            expects=rejects("E0521"),
            note="a bound expression cannot leave the closure that minted it",
        ),
        _pipeline(
            "binder-brand-crossed",
            nested,
            verdict="reject",
            phase="borrowck",
            # The brand is invariant and introduced by a higher-ranked bound, so
            # the mismatch surfaces as a region error rustc assigns no code to.
            # An expectation that insisted on a code could not express this
            # rejection at all, and the surface would go uncovered.
            expects=rejects(message="lifetime may not live long enough"),
            note="one pipeline's binder cannot satisfy another's closure",
        ),
    ]


# [spec:pgorm:req:generative.compile-suite]
def cases():
    return (
        transaction_accept_cases()
        + transaction_reject_cases()
        + binder_accept_cases()
        + binder_reject_cases()
    )


__all__ = ["OBLIGATION", "cases"]
