#!/bin/bash
set -e

find examples/ -depth -type f -name '*.toml' -exec sed -i '/^path = "..\/..\/..\/pgorm-migration"/d' {} \;
find examples/ -depth -type f -name '*.toml' -exec sed -i '/^path = "..\/..\/..\/"/d' {} \;