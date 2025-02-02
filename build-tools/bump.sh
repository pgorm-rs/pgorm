#!/bin/bash
set -e

# Bump `pgorm-codegen` version
cd pgorm-codegen
sed -i 's/^version.*$/version = "'$1'"/' Cargo.toml
cd ..

# Bump `pgorm-cli` version
cd pgorm-cli
sed -i 's/^version.*$/version = "'$1'"/' Cargo.toml
sed -i 's/^pgorm-codegen [^,]*,/pgorm-codegen = { version = "\='$1'",/' Cargo.toml
cd ..

# Bump `pgorm-macros` version
cd pgorm-macros
sed -i 's/^version.*$/version = "'$1'"/' Cargo.toml
cd ..

# Bump `pgorm` version
sed -i 's/^version.*$/version = "'$1'"/' Cargo.toml
sed -i 's/^pgorm-macros [^,]*,/pgorm-macros = { version = "'~$1'",/' Cargo.toml

# Bump `pgorm-migration` version
cd pgorm-migration
sed -i 's/^version.*$/version = "'$1'"/' Cargo.toml
sed -i 's/^pgorm-cli [^,]*,/pgorm-cli = { version = "'~$1'",/' Cargo.toml
sed -i 's/^pgorm [^,]*,/pgorm = { version = "'~$1'",/' Cargo.toml
cd ..

git commit -am "$1"

# Bump examples' dependency version
cd examples
find . -depth -type f -name '*.toml' -exec sed -i 's/^version = ".*" # pgorm version$/version = "'~$1'" # pgorm version/' {} \;
find . -depth -type f -name '*.toml' -exec sed -i 's/^version = ".*" # pgorm-migration version$/version = "'~$1'" # pgorm-migration version/' {} \;
git add .
git commit -m "update examples"