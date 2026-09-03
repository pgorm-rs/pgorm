use crate::{
    ActiveEnum, Column, ConjunctRelation, Entity, EntityWriter, Error, PrimaryKey, Relation,
    RelationType,
};
use pgorm_query::{ColumnSpec, TableCreateStatement};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug)]
pub struct EntityTransformer;

impl EntityTransformer {
    // [spec:pgorm:sem:codegen.entity.transform+5]
    // [spec:pgorm:sem:codegen.entity.transform.inverse]
    // [spec:pgorm:sem:codegen.entity.transform.conjunct]
    pub fn transform(table_create_stmts: Vec<TableCreateStatement>) -> Result<EntityWriter, Error> {
        let mut enums: BTreeMap<String, ActiveEnum> = BTreeMap::new();
        let mut inverse_relations: BTreeMap<String, Vec<Relation>> = BTreeMap::new();
        let mut entities = BTreeMap::new();
        for table_create in table_create_stmts.into_iter() {
            let table_name = table_create.get_table_name().table().to_string();
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
                col.unique = table_create
                    .get_indexes()
                    .iter()
                    .filter(|index| index.is_unique_key())
                    .map(|index| index.get_index_spec().get_column_names())
                    .filter(|col_names| col_names.len() == 1 && col_names[0] == col.name)
                    .count()
                    > 0;
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
                let mut unique = true;
                for column in rel.columns.iter() {
                    if !entity
                        .columns
                        .iter()
                        .filter(|col| col.unique)
                        .any(|col| col.name.as_str() == column)
                    {
                        unique = false;
                        break;
                    }
                }
                if rel.columns.len() == entity.primary_keys.len() {
                    let mut count_pk = 0;
                    for primary_key in entity.primary_keys.iter() {
                        if rel.columns.contains(&primary_key.name) {
                            count_pk += 1;
                        }
                    }
                    if count_pk == entity.primary_keys.len() {
                        unique = true;
                    }
                }
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
            if entity.primary_keys.len() != 2 {
                continue;
            }
            let [one, other] = entity.relations.as_slice() else {
                continue;
            };
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
                    &Default::default(),
                    false,
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
