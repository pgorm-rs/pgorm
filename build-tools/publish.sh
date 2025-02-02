#!/bin/bash
set -e

# publish `pgorm-codegen`
cd pgorm-codegen
cargo publish
cd ..

# publish `pgorm-cli`
cd pgorm-cli
cargo publish
cd ..

# publish `pgorm-macros`
cd pgorm-macros
cargo publish
cd ..

# publish `pgorm`
cargo publish

# publish `pgorm-migration`
cd pgorm-migration
cargo publish