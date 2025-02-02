rm -rf pgorm-macros/src/strum/helpers
rm -rf pgorm-macros/src/strum/enum_iter.rs

cp -r ../strum/strum_macros/src/helpers pgorm-macros/src/strum/helpers
cp -r ../strum/strum_macros/src/macros/enum_iter.rs pgorm-macros/src/strum/enum_iter.rs

sed -i 's/crate::helpers::{*/super::helpers::{/' pgorm-macros/src/strum/enum_iter.rs
sed -i 's/parse_quote!(::strum)*/parse_quote!(pgorm::strum)/' pgorm-macros/src/strum/helpers/type_props.rs