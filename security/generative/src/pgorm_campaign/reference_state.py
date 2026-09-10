"""Observe all disposable user schemas, definitions and rows with a separate driver."""

from .comparison import InvalidOracle, encoded, row_key
from .reference_sql import SQL
from .reference_values import qualified

USER_SCHEMA = "n.nspname NOT LIKE 'pg_%' AND n.nspname <> 'information_schema'"
DEFINITIONS = {
    "schemas": "SELECT n.nspname FROM pg_namespace n WHERE "
    + USER_SCHEMA
    + " ORDER BY 1",
    "columns": """
        SELECT n.nspname, c.relname, a.attnum, a.attname,
               tn.nspname, t.typname, a.atttypmod, a.attnotnull,
               a.attidentity, a.attgenerated, pg_get_expr(d.adbin, d.adrelid)
        FROM pg_attribute a JOIN pg_class c ON c.oid = a.attrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace
        JOIN pg_type t ON t.oid = a.atttypid
        JOIN pg_namespace tn ON tn.oid = t.typnamespace
        LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
        WHERE a.attnum > 0 AND NOT a.attisdropped AND c.relkind IN ('r','p','v','m','f') AND
    """
    + USER_SCHEMA
    + " ORDER BY 1,2,3",
    "constraints": """
        SELECT n.nspname, c.relname, k.conname, k.contype,
               pg_get_constraintdef(k.oid, true), k.convalidated
        FROM pg_constraint k JOIN pg_class c ON c.oid = k.conrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace WHERE
    """
    + USER_SCHEMA
    + " ORDER BY 1,2,3",
    "indexes": """
        SELECT n.nspname, c.relname, i.relname, pg_get_indexdef(x.indexrelid),
               x.indisvalid, x.indisready
        FROM pg_index x JOIN pg_class c ON c.oid = x.indrelid
        JOIN pg_class i ON i.oid = x.indexrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace WHERE
    """
    + USER_SCHEMA
    + " ORDER BY 1,2,3",
    "enums": """
        SELECT n.nspname, t.typname, e.enumsortorder, e.enumlabel
        FROM pg_type t JOIN pg_namespace n ON n.oid = t.typnamespace
        LEFT JOIN pg_enum e ON e.enumtypid = t.oid WHERE t.typtype = 'e' AND
    """
    + USER_SCHEMA
    + " ORDER BY 1,2,3",
    "views": """
        SELECT n.nspname, c.relname, pg_get_viewdef(c.oid, true)
        FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
        WHERE c.relkind IN ('v','m') AND
    """
    + USER_SCHEMA
    + " ORDER BY 1,2",
    "triggers": """
        SELECT n.nspname, c.relname, t.tgname, pg_get_triggerdef(t.oid, true), t.tgenabled
        FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid
        JOIN pg_namespace n ON n.oid = c.relnamespace WHERE NOT t.tgisinternal AND
    """
    + USER_SCHEMA
    + " ORDER BY 1,2,3",
    "routines": """
        SELECT n.nspname, p.proname, pg_get_function_identity_arguments(p.oid),
               pg_get_functiondef(p.oid)
        FROM pg_proc p JOIN pg_namespace n ON n.oid = p.pronamespace
        WHERE p.prokind IN ('f','p') AND
    """
    + USER_SCHEMA
    + " ORDER BY 1,2,3",
    "sequences": """
        SELECT schemaname, sequencename, data_type::text, start_value, min_value,
               max_value, increment_by, cycle, cache_size, last_value
        FROM pg_sequences WHERE schemaname NOT LIKE 'pg_%'
        ORDER BY 1,2
    """,
}

RELATIONS = (
    """
    SELECT n.nspname, c.relname, c.relkind, c.relrowsecurity, c.relforcerowsecurity
    FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
    WHERE c.relkind IN ('r','p','v','m','f') AND
"""
    + USER_SCHEMA
    + " ORDER BY 1,2"
)


async def catalog(connection, text):
    async with connection.cursor() as cursor:
        await cursor.execute(text)
        return [list(row) for row in await cursor.fetchall()]


# [spec:pgorm:req:generative.comparison]
async def snapshot(driver, *, row_limit=10000):
    result = {}
    async with driver.connection.transaction():
        await driver.connection.execute(
            "SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY"
        )
        for name, text in DEFINITIONS.items():
            result[name] = await catalog(driver.connection, text)
        result["relations"] = await catalog(driver.connection, RELATIONS)
        tables = []
        for schema, name, kind, _, _ in result["relations"]:
            if kind not in ("r", "p", "m"):
                raise InvalidOracle(
                    "fixture contains a relation with uncovered state semantics: "
                    + kind
                )
            records, _ = await driver.query(
                SQL(
                    (
                        "SELECT * FROM "
                        + qualified(name, schema)
                        + " LIMIT "
                        + str(row_limit + 1),
                    )
                )
            )
            if len(records) > row_limit:
                raise InvalidOracle("fixture state exceeded the observation row budget")
            tables.append(
                {
                    "schema": schema,
                    "name": name,
                    "rows": sorted(encoded(row_key(record)) for record in records),
                }
            )
        result["tables"] = tables
    return result
