use crate::{
    ActiveEnum, Column, ConjunctRelation, Entity, EntityWriter, Error, PrimaryKey, Relation,
    RelationType, util::escape_rust_keyword,
};
use heck::{ToSnakeCase, ToUpperCamelCase};
use pgorm_query::{ColumnSpec, TableCreateStatement};
use std::collections::{BTreeMap, BTreeSet, HashMap};

#[derive(Clone, Debug)]
pub struct EntityTransformer;

impl EntityTransformer {
    // [spec:pgorm:sem:codegen.entity.transform+6]
    // [spec:pgorm:sem:codegen.entity.transform.inverse+1]
    // [spec:pgorm:sem:codegen.entity.transform.conjunct+1]
    // [spec:pgorm:req:codegen.entity.collisions]
    pub fn transform(table_create_stmts: Vec<TableCreateStatement>) -> Result<EntityWriter, Error> {
        let mut enums: BTreeMap<String, ActiveEnum> = BTreeMap::new();
        let mut inverse_relations: BTreeMap<String, Vec<Relation>> = BTreeMap::new();
        let mut entities = BTreeMap::new();
        for table_create in table_create_stmts.into_iter() {
            let table_name = table_create.get_table_name().table().to_string();
            let unique_column_sets: Vec<BTreeSet<String>> = table_create
                .get_indexes()
                .iter()
                .filter(|index| index.is_unique_key())
                .map(|index| {
                    index
                        .get_index_spec()
                        .get_column_names()
                        .into_iter()
                        .collect()
                })
                .collect();
            let mut primary_keys: Vec<PrimaryKey> = Vec::new();
            let mut columns: Vec<Column> = Vec::new();
            for col_def in table_create.get_columns() {
                let primary_key = col_def
                    .get_column_spec()
                    .iter()
                    .any(|spec| matches!(spec, ColumnSpec::PrimaryKey));
                if primary_key {
                    primary_keys.push(PrimaryKey {
                        name: col_def.get_column_name(),
                    });
                }
                let mut col = Column::try_from(col_def).map_err(|err| err.in_table(&table_name))?;
                col.unique = col.unique
                    || unique_column_sets
                        .iter()
                        .any(|columns| columns.len() == 1 && columns.contains(&col.name));
                if let pgorm_query::ColumnType::Enum { name, variants } = col.get_inner_col_type() {
                    enums.insert(
                        name.to_string(),
                        ActiveEnum {
                            enum_name: name.clone(),
                            values: variants.clone(),
                        },
                    );
                }
                columns.push(col);
            }
            let mut ref_table_counts: BTreeMap<String, usize> = BTreeMap::new();
            let foreign_keys: Vec<Relation> = table_create
                .get_foreign_key_create_stmts()
                .iter()
                .map(|fk_create_stmt| Relation::from(fk_create_stmt.get_foreign_key()))
                .collect();
            for relation in foreign_keys.iter() {
                if let Some(count) = ref_table_counts.get_mut(&relation.ref_table) {
                    if *count == 0 {
                        *count = 1;
                    }
                    *count += 1;
                } else {
                    ref_table_counts.insert(relation.ref_table.clone(), 0);
                }
            }
            let relations: Vec<Relation> = foreign_keys
                .into_iter()
                .rev()
                .map(|mut rel: Relation| {
                    rel.self_referencing = rel.ref_table == table_name;
                    if let Some(count) = ref_table_counts.get_mut(&rel.ref_table) {
                        rel.num_suffix = *count;
                        if *count > 0 {
                            *count -= 1;
                        }
                    }
                    rel
                })
                .rev()
                .collect();
            primary_keys.extend(
                table_create
                    .get_indexes()
                    .iter()
                    .filter(|index| index.is_primary_key())
                    .flat_map(|index| {
                        index
                            .get_index_spec()
                            .get_column_names()
                            .into_iter()
                            .map(|name| PrimaryKey { name })
                            .collect::<Vec<_>>()
                    }),
            );
            let entity = Entity {
                table_name: table_name.clone(),
                columns,
                relations: relations.clone(),
                conjunct_relations: vec![],
                primary_keys,
            };
            entities.insert(table_name.clone(), entity.clone());
            for mut rel in relations.into_iter() {
                // This will produce a duplicated relation
                if rel.self_referencing {
                    continue;
                }
                // This will cause compile error on the many side,
                // got relation variant but without Related<T> implemented
                if rel.num_suffix > 0 {
                    continue;
                }
                let ref_table = rel.ref_table;
                let key_columns: BTreeSet<String> = rel.columns.iter().cloned().collect();
                let every_column_unique = rel.columns.iter().all(|column| {
                    entity
                        .columns
                        .iter()
                        .filter(|col| col.unique)
                        .any(|col| col.name.as_str() == column)
                });
                // a unique constraint over exactly the key's columns constrains
                // the key as a whole, which one-column-at-a-time cannot see
                let key_is_unique = unique_column_sets.contains(&key_columns);
                let key_is_primary = key_columns.len() == entity.primary_keys.len()
                    && entity
                        .primary_keys
                        .iter()
                        .all(|primary_key| key_columns.contains(&primary_key.name));
                let unique = every_column_unique || key_is_unique || key_is_primary;
                let rel_type = if unique {
                    RelationType::HasOne
                } else {
                    RelationType::HasMany
                };
                rel.rel_type = rel_type;
                rel.ref_table = table_name.to_string();
                rel.columns = Vec::new();
                rel.ref_columns = Vec::new();
                if let Some(vec) = inverse_relations.get_mut(&ref_table) {
                    vec.push(rel);
                } else {
                    inverse_relations.insert(ref_table, vec![rel]);
                }
            }
        }
        validate_references(&entities)?;
        validate_identities(&entities, &enums)?;
        for (tbl_name, relations) in inverse_relations.into_iter() {
            if let Some(entity) = entities.get_mut(&tbl_name) {
                for relation in relations.into_iter() {
                    let duplicate_relation = entity
                        .relations
                        .iter()
                        .any(|rel| rel.ref_table == relation.ref_table);
                    if !duplicate_relation {
                        entity.relations.push(relation);
                    }
                }
            }
        }
        let mut conjunct_relations: Vec<(String, ConjunctRelation)> = Vec::new();
        for (table_name, entity) in entities.iter() {
            // only the table's own foreign keys are junction legs; the inverse
            // relations synthesised above are relations of other tables' keys
            let owned: Vec<&Relation> = entity
                .relations
                .iter()
                .filter(|rel| {
                    matches!(rel.rel_type, RelationType::BelongsTo) && !rel.columns.is_empty()
                })
                .collect();
            let [one, other] = owned.as_slice() else {
                continue;
            };
            let key_columns: BTreeSet<&String> =
                one.columns.iter().chain(other.columns.iter()).collect();
            let primary_keys: BTreeSet<&String> = entity
                .primary_keys
                .iter()
                .map(|primary_key| &primary_key.name)
                .collect();
            if key_columns != primary_keys {
                continue;
            }
            for (rel, another_rel) in [(one, other), (other, one)] {
                conjunct_relations.push((
                    rel.ref_table.clone(),
                    ConjunctRelation {
                        via: table_name.clone(),
                        to: another_rel.ref_table.clone(),
                    },
                ));
            }
        }
        for (ref_table, conjunct_relation) in conjunct_relations {
            if let Some(entity) = entities.get_mut(&ref_table) {
                entity.conjunct_relations.push(conjunct_relation);
            }
        }
        let entities: Vec<Entity> = entities
            .into_values()
            .map(|mut v| {
                // Filter duplicated conjunct relations
                let duplicated_to: Vec<_> = v
                    .conjunct_relations
                    .iter()
                    .fold(HashMap::new(), |mut acc, conjunct_relation| {
                        acc.entry(conjunct_relation.to.clone())
                            .and_modify(|c| *c += 1)
                            .or_insert(1);
                        acc
                    })
                    .into_iter()
                    .filter(|(_, v)| v > &1)
                    .map(|(k, _)| k)
                    .collect();
                v.conjunct_relations
                    .retain(|conjunct_relation| !duplicated_to.contains(&conjunct_relation.to));

                // Skip `impl Related ... { fn to() ... }` implementation block,
                // if the same related entity is being referenced by a conjunct relation
                v.relations.iter_mut().for_each(|relation| {
                    if v.conjunct_relations
                        .iter()
                        .any(|conjunct_relation| conjunct_relation.to == relation.ref_table)
                    {
                        relation.impl_related = false;
                    }
                });

                // Sort relation vectors
                v.relations.sort_by(|a, b| a.ref_table.cmp(&b.ref_table));
                v.conjunct_relations.sort_by(|a, b| a.to.cmp(&b.to));
                v
            })
            .collect();
        for entity in entities.iter() {
            entity.validate()?;
        }
        for active_enum in enums.values() {
            active_enum.validate()?;
        }
        Ok(EntityWriter { entities, enums })
    }
}

/// Every relation joins tables and columns this schema has: a generated file
/// names its target's module and columns, so a foreign key onto a table the
/// caller did not pass would generate Rust that does not compile.
// [spec:pgorm:sem:codegen.entity.transform+6]
fn validate_references(entities: &BTreeMap<String, Entity>) -> Result<(), Error> {
    for (table_name, entity) in entities.iter() {
        for relation in entity.relations.iter() {
            let ref_table = relation.ref_table.as_str();
            let context = format!("table `{table_name}`: relation to `{ref_table}`");
            let Some(ref_entity) = entities.get(ref_table) else {
                return Err(Error::TransformError(format!(
                    "{context} names a table the schema does not define"
                )));
            };
            for column in relation.columns.iter() {
                if !entity.columns.iter().any(|col| &col.name == column) {
                    return Err(Error::TransformError(format!(
                        "{context} constrains column `{column}`, which the table does not have"
                    )));
                }
            }
            for ref_column in relation.ref_columns.iter() {
                if !ref_entity.columns.iter().any(|col| &col.name == ref_column) {
                    return Err(Error::TransformError(format!(
                        "{context} references column `{ref_column}`, which `{ref_table}` does not \
                         have"
                    )));
                }
            }
        }
    }
    Ok(())
}

/// Every identity the writer derives from a DB name belongs to one name alone.
/// Two tables that derive one module name would write one file over the other
/// and declare the module twice; two columns of a table, two enums or two
/// values of one enum that derive one identifier would each emit a duplicate
/// definition.
// [spec:pgorm:req:codegen.entity.collisions]
fn validate_identities(
    entities: &BTreeMap<String, Entity>,
    enums: &BTreeMap<String, ActiveEnum>,
) -> Result<(), Error> {
    let mut modules = Claims::default();
    let mut types = Claims::default();
    for (table_name, entity) in entities.iter() {
        modules.claim(
            "tables",
            "module name",
            escape_rust_keyword(entity.get_table_name_snake_case()),
            table_name,
        )?;
        types.claim(
            "tables",
            "type name",
            escape_rust_keyword(entity.get_table_name_camel_case()),
            table_name,
        )?;
        let mut fields = Claims::default();
        let whose = format!("table `{table_name}` columns");
        for column in entity.columns.iter() {
            fields.claim(
                &whose,
                "field name",
                escape_rust_keyword(column.name.to_snake_case()),
                &column.name,
            )?;
        }
    }
    let mut enum_types = Claims::default();
    for (enum_name, active_enum) in enums.iter() {
        enum_types.claim(
            "enums",
            "type name",
            enum_name.to_upper_camel_case(),
            enum_name,
        )?;
        let mut variants = Claims::default();
        let whose = format!("enum `{enum_name}` values");
        for (value, variant) in active_enum.variant_names() {
            variants.claim(&whose, "variant name", variant, &value)?;
        }
    }
    Ok(())
}

/// The DB name that claimed each derived identifier, so a second claim on one
/// can name both sides.
// [spec:pgorm:req:codegen.entity.collisions]
#[derive(Default)]
struct Claims(HashMap<String, String>);

impl Claims {
    /// Record `source`'s claim on `derived`, or report the name that claimed it
    /// first. `whose` opens the message with the names in play (`tables`,
    /// ``table `cake` columns``) and `what` names the identifier they share.
    fn claim(
        &mut self,
        whose: &str,
        what: &str,
        derived: String,
        source: &str,
    ) -> Result<(), Error> {
        if let Some(first) = self.0.insert(derived.clone(), source.to_owned()) {
            return Err(Error::TransformError(format!(
                "{whose} `{first}` and `{source}` both generate the {what} `{derived}`"
            )));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pgorm::Schema;
    use pretty_assertions::assert_eq;
    use proc_macro2::TokenStream;
    use std::{
        error::Error,
        io::{self, BufRead, BufReader},
    };

    #[test]
    fn duplicated_many_to_many_paths() -> Result<(), Box<dyn Error>> {
        use crate::tests_cfg::duplicated_many_to_many_paths::*;
        let schema = Schema::new();

        validate_compact_entities(
            vec![
                schema.create_table_from_entity(bills::Entity),
                schema.create_table_from_entity(users::Entity),
                schema.create_table_from_entity(users_saved_bills::Entity),
                schema.create_table_from_entity(users_votes::Entity),
            ],
            vec![
                (
                    "bills",
                    include_str!("../tests_cfg/duplicated_many_to_many_paths/bills.rs"),
                ),
                (
                    "users",
                    include_str!("../tests_cfg/duplicated_many_to_many_paths/users.rs"),
                ),
                (
                    "users_saved_bills",
                    include_str!("../tests_cfg/duplicated_many_to_many_paths/users_saved_bills.rs"),
                ),
                (
                    "users_votes",
                    include_str!("../tests_cfg/duplicated_many_to_many_paths/users_votes.rs"),
                ),
            ],
        )
    }

    #[test]
    fn many_to_many() -> Result<(), Box<dyn Error>> {
        use crate::tests_cfg::many_to_many::*;
        let schema = Schema::new();

        validate_compact_entities(
            vec![
                schema.create_table_from_entity(bills::Entity),
                schema.create_table_from_entity(users::Entity),
                schema.create_table_from_entity(users_votes::Entity),
            ],
            vec![
                ("bills", include_str!("../tests_cfg/many_to_many/bills.rs")),
                ("users", include_str!("../tests_cfg/many_to_many/users.rs")),
                (
                    "users_votes",
                    include_str!("../tests_cfg/many_to_many/users_votes.rs"),
                ),
            ],
        )
    }

    #[test]
    fn many_to_many_multiple() -> Result<(), Box<dyn Error>> {
        use crate::tests_cfg::many_to_many_multiple::*;
        let schema = Schema::new();

        validate_compact_entities(
            vec![
                schema.create_table_from_entity(bills::Entity),
                schema.create_table_from_entity(users::Entity),
                schema.create_table_from_entity(users_votes::Entity),
            ],
            vec![
                (
                    "bills",
                    include_str!("../tests_cfg/many_to_many_multiple/bills.rs"),
                ),
                (
                    "users",
                    include_str!("../tests_cfg/many_to_many_multiple/users.rs"),
                ),
                (
                    "users_votes",
                    include_str!("../tests_cfg/many_to_many_multiple/users_votes.rs"),
                ),
            ],
        )
    }

    #[test]
    fn self_referencing() -> Result<(), Box<dyn Error>> {
        use crate::tests_cfg::self_referencing::*;
        let schema = Schema::new();

        validate_compact_entities(
            vec![
                schema.create_table_from_entity(bills::Entity),
                schema.create_table_from_entity(users::Entity),
            ],
            vec![
                (
                    "bills",
                    include_str!("../tests_cfg/self_referencing/bills.rs"),
                ),
                (
                    "users",
                    include_str!("../tests_cfg/self_referencing/users.rs"),
                ),
            ],
        )
    }

    fn validate_compact_entities(
        table_create_stmts: Vec<TableCreateStatement>,
        files: Vec<(&str, &str)>,
    ) -> Result<(), Box<dyn Error>> {
        let entities: HashMap<_, _> = EntityTransformer::transform(table_create_stmts)?
            .entities
            .into_iter()
            .map(|entity| (entity.table_name.clone(), entity))
            .collect();

        for (entity_name, file_content) in files {
            let entity = entities
                .get(entity_name)
                .expect("Forget to add entity to the list");

            assert_eq!(
                parse_from_file(file_content.as_bytes())?.to_string(),
                EntityWriter::gen_compact_code_blocks(
                    entity,
                    &crate::WithSerde::None,
                    &crate::DateTimeCrate::Chrono,
                    &None,
                    false,
                    false,
                    &Default::default(),
                    &Default::default()
                )
                .into_iter()
                .skip(1)
                .fold(TokenStream::new(), |mut acc, tok| {
                    acc.extend(tok);
                    acc
                })
                .to_string()
            );
        }

        Ok(())
    }

    fn parse_from_file<R>(inner: R) -> io::Result<TokenStream>
    where
        R: io::Read,
    {
        let mut reader = BufReader::new(inner);
        let mut lines: Vec<String> = Vec::new();

        reader.read_until(b';', &mut Vec::new())?;

        let mut line = String::new();
        while reader.read_line(&mut line)? > 0 {
            lines.push(line.to_owned());
            line.clear();
        }
        let content = lines.join("");
        Ok(content.parse().unwrap())
    }
}
