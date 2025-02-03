#!/bin/bash
set -e
if [ -d ./build-tools ]; then
    targets=(
        "Cargo.toml --all-features"
        "pgorm-query-attr/Cargo.toml"
        "pgorm-query-binder/Cargo.toml"
        "pgorm-query-derive/Cargo.toml"
        "pgorm-query-postgres/Cargo.toml"
        "pgorm-query-rusqlite/Cargo.toml"
    )

    for target in "${targets[@]}"; do
        echo "cargo clippy --manifest-path ${target} --fix --allow-dirty --allow-staged"
        cargo clippy --manifest-path ${target} --fix --allow-dirty --allow-staged
    done

    examples=(`find examples -type f -name 'Cargo.toml'`)
    for example in "${examples[@]}"; do
        echo "cargo clippy --manifest-path ${example} --fix --allow-dirty --allow-staged"
        cargo clippy --manifest-path "${example}" --fix --allow-dirty --allow-staged
    done
else
    echo "Please execute this script from the repository root."
fi
