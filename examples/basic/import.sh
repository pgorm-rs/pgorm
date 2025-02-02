cp ../../src/tests_cfg/cake.rs src/example_cake.rs
cp ../../src/tests_cfg/fruit.rs src/example_fruit.rs
cp ../../src/tests_cfg/filling.rs src/example_filling.rs
cp ../../src/tests_cfg/cake_filling.rs src/example_cake_filling.rs

sed -i 's/^use crate::/use pgorm::/g' src/*.rs
sed -i '/^use crate as pgorm;/d' src/*.rs