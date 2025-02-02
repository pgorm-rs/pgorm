#!/bin/bash
set -e
if [ -d ./build-tools ]; then
    targets=(
        "Cargo.toml"
        "pgorm-cli/Cargo.toml"
        "pgorm-codegen/Cargo.toml"
        "pgorm-macros/Cargo.toml"
        "pgorm-migration/Cargo.toml"
        "pgorm-rocket/Cargo.toml"
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

    slmd COMMUNITY.md -oi
else
    echo "Please execute this script from the repository root."
fi
