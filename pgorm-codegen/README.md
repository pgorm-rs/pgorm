# pgorm-codegen

Entity source generation for pgorm. Give it a schema — introspected from a
live database, or parsed from DDL text via
[libpg_query](https://github.com/pganalyze/libpg_query) (`entities_from_sql`)
— and it writes the entity modules: models, columns, primary keys, relations,
and active enums. Library-only; there is no CLI. Call it from a build script
or a small binary of your own.
