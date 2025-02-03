#!/bin/bash
set -e
if [ -d ./build-tools ]; then
    targets=(
        "Cargo.toml"
        "pgorm-query-attr/Cargo.toml"
        "pgorm-query-binder/Cargo.toml"
        "pgorm-query-derive/Cargo.toml"
        "pgorm-query-postgres/Cargo.toml"
        "pgorm-query-rusqlite/Cargo.toml"
    )

    for target in "${targets[@]}"; do
        echo "cargo +nightly fmt --manifest-path ${target} --all"
        cargo +nightly fmt --manifest-path "${target}" --all
    done

    examples=(`find examples -type f -name 'Cargo.toml'`)
    for example in "${examples[@]}"; do
        echo "cargo +nightly fmt --manifest-path ${example} --all"
        cargo +nightly fmt --manifest-path "${example}" --all
    done
else
    echo "Please execute this script from the repository root."
fi
