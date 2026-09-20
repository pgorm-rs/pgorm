# query-name-vocabulary — in flight

Node: `query-name-vocabulary` (nplan). Agent session `opus-m7`.

## Decided mapping

| old | new |
|---|---|
| `SharedIden` / `DynIden` | `Name` (one exported name, `Arc` private) |
| `trait Iden` | `trait SqlName` |
| `trait IntoIden` / `into_iden` | `IntoName` / `into_name` |
| `IdenStatic` + pgorm's `IdenStr` | one `StaticName` (in pgorm-query) |
| `Alias(String)` / `Alias::new(x)` | `Name::runtime(x)`, private `RuntimeName` |
| free `alias()` / `AliasName` | unchanged |
| pgorm `Identity` / `IntoIdentity` / `IdentityOf` | `Key` / `IntoKey` / `KeyOf` |
| derives `Iden` / `IdenStatic` | `SqlName` / `StaticName` |
| pgorm `DeriveIden` | `DeriveSqlName` |

`StaticName: SqlName + Copy + Debug + 'static { fn as_str(&self) -> &str }`
— the looser `&str` return (pgorm's `IdenStr` shape) wins so `EntityName::table_name`
does not have to become `&'static str`; `Debug` (pgorm's shape) wins so the entity
trait family keeps its bound.

## Layers

- [x] L1 rename the Name family + unify `StaticName` + `Identity` → `Key`
      (`Alias` retained; `IntoName for &str/String` retained)
- [x] L2 `Alias::new` → `Name::runtime`, delete `IntoName for &str/String`
      and `IntoKey for &str/String` (replaced by `IntoKey for Name`)
- [ ] L3 spec pass (bump + repin), node completion, plan residue

The `#[iden = ..]` / `#[pgorm(iden = ..)]` derive attribute is deliberately NOT
renamed: it is not in the ratified mapping, and `#[pgorm(name = ..)]` would sit
next to `column_name` / `enum_name` / `table_name` keys that mean other things.

## Forced design call, L1

Unifying `IdenStr` into pgorm-query's `StaticName` makes the trait foreign to
pgorm, and a blanket `impl<T: StaticName> IntoKey for T` in pgorm then conflicts
with every concrete impl (`String`, `&str`, every tuple arity) — rustc refuses
negative reasoning about foreign-trait impls. The blanket must live in the crate
that owns the trait, so `Key`, `IntoKey` and `IntoBoundary` moved to
`pgorm-query/src/key.rs`; pgorm re-exports them and keeps `ColumnPairs` + `KeyOf`
(which need `EntityTrait`) in `src/entity/key.rs`.

## Verification per layer

`cargo check --workspace --all-targets`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo nextest run -p pgorm-query`, workspace nextest (baseline 1449/1449 at 6f1d1b74),
`nplan check`, migration template, findings reproducers.
`cargo test --doc --workspace` ONCE at the end, alone.
