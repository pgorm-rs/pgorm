#!/bin/bash
set -e

# publish `pgorm-query-attr`
cd pgorm-query-attr
cargo publish
cd ..

# publish `pgorm-query-derive`
cd pgorm-query-derive
cargo publish
cd ..

# publish `pgorm-query`
cargo publish

# publish `pgorm-query-binder`
cd pgorm-query-binder
cargo publish
cd ..

# publish `pgorm-query-rusqlite`
cd pgorm-query-rusqlite
cargo publish
cd ..

# publish `pgorm-query-postgres`
cd pgorm-query-postgres
cargo publish
cd ..
