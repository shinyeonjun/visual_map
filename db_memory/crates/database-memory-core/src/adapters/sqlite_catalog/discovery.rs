impl RawSqliteCatalog {
    fn read(conn: &Connection) -> Result<Self, SqliteAdapterError> {
        let schema_entries = read_schema_entries(conn)?;
        let database_names = read_database_names(conn)?;
        let schema_version = conn.query_row("PRAGMA main.schema_version", [], |row| row.get(0))?;
        let mut relations = read_relations(conn, &schema_entries)?;
        let mut indexes = Vec::new();
        let mut foreign_keys = BTreeMap::new();
        for relation in relations
            .iter_mut()
            .filter(|relation| relation.kind.is_table())
        {
            relation.columns = read_relation_columns(conn, &relation.name)?;
            if relation.columns.len() != relation.declared_column_count as usize {
                return Err(SqliteAdapterError::mapping(
                    format!("table {}", relation.name),
                    format!(
                        "PRAGMA table_list reports {} column(s), but table_xinfo returned {}",
                        relation.declared_column_count,
                        relation.columns.len()
                    ),
                ));
            }
            relation.parsed_table = parse_relation_table(relation)?;
            indexes.extend(read_indexes(conn, &relation.name, &schema_entries)?);
            foreign_keys.insert(
                relation.name.clone(),
                read_foreign_keys(conn, &relation.name)?,
            );
        }
        for relation in relations
            .iter_mut()
            .filter(|relation| relation.kind == RawRelationKind::View)
        {
            relation.columns = read_relation_columns(conn, &relation.name)?;
            if relation.columns.len() != relation.declared_column_count as usize {
                return Err(SqliteAdapterError::mapping(
                    format!("view {}", relation.name),
                    format!(
                        "PRAGMA table_list reports {} column(s), but table_xinfo returned {}",
                        relation.declared_column_count,
                        relation.columns.len()
                    ),
                ));
            }
        }
        let triggers = read_triggers(&schema_entries)?;

        Ok(Self {
            sqlite_version: rusqlite::version().to_owned(),
            schema_version,
            database_names,
            relations,
            indexes,
            foreign_keys,
            triggers,
        })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RawRelationKind {
    Table(TableKind),
    View,
}

impl RawRelationKind {
    fn is_table(self) -> bool {
        matches!(self, Self::Table(_))
    }
}

#[derive(Clone, Debug)]
struct RawRelation {
    name: String,
    kind: RawRelationKind,
    declared_column_count: u32,
    without_rowid: bool,
    strict: bool,
    sql: Option<String>,
    columns: Vec<RawColumn>,
    parsed_table: Option<ParsedTableDefinition>,
}

#[derive(Clone, Debug)]
struct RawColumn {
    cid: i64,
    name: String,
    data_type: String,
    not_null: bool,
    default_value: Option<String>,
    primary_key_position: u32,
    hidden: i64,
}

#[derive(Clone, Debug)]
struct RawIndex {
    table_name: String,
    name: String,
    unique: bool,
    origin: String,
    partial: bool,
    sql: Option<String>,
    parsed: Option<ParsedIndexDefinition>,
    terms: Vec<RawIndexTerm>,
}

#[derive(Clone, Debug)]
struct RawIndexTerm {
    sequence: u32,
    cid: i64,
    column_name: Option<String>,
    descending: bool,
    collation: Option<String>,
    key: bool,
}

#[derive(Clone, Debug)]
struct RawForeignKey {
    id: i64,
    parts: Vec<RawForeignKeyPart>,
    on_update: String,
    on_delete: String,
    match_name: String,
}

#[derive(Clone, Debug)]
struct RawForeignKeyPart {
    sequence: u32,
    referenced_table: String,
    source_column: String,
    referenced_column: Option<String>,
}

#[derive(Clone, Debug)]
struct RawTrigger {
    name: String,
    owner_name: String,
    sql: String,
    parsed: ParsedTriggerDefinition,
}

#[derive(Clone, Debug)]
struct RawSchemaEntry {
    object_type: String,
    name: String,
    owner_name: String,
    sql: Option<String>,
}

fn read_schema_entries(
    conn: &Connection,
) -> Result<BTreeMap<(String, String), RawSchemaEntry>, SqliteAdapterError> {
    let mut stmt = conn
        .prepare("SELECT type, name, tbl_name, sql FROM main.sqlite_schema ORDER BY type, name")?;
    let rows = stmt.query_map([], |row| {
        Ok(RawSchemaEntry {
            object_type: row.get(0)?,
            name: row.get(1)?,
            owner_name: row.get(2)?,
            sql: row.get(3)?,
        })
    })?;
    let mut entries = BTreeMap::new();
    for row in rows {
        let entry = row?;
        if let Some(sql) = entry.sql.as_deref() {
            require_bounded_sql(&entry.object_type, &entry.name, sql)?;
        }
        entries.insert((entry.object_type.clone(), entry.name.clone()), entry);
    }
    Ok(entries)
}

fn read_database_names(conn: &Connection) -> Result<Vec<String>, SqliteAdapterError> {
    let mut stmt = conn.prepare("PRAGMA database_list")?;
    let rows = stmt.query_map([], |row| row.get::<_, String>(1))?;
    let names = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    if names != [MAIN_CATALOG.to_owned()] {
        return Err(SqliteAdapterError::mapping(
            "database scope",
            format!(
                "certified SQLite introspection expected only the main database, found {}",
                names.join(", ")
            ),
        ));
    }
    Ok(names)
}

fn read_relations(
    conn: &Connection,
    schema_entries: &BTreeMap<(String, String), RawSchemaEntry>,
) -> Result<Vec<RawRelation>, SqliteAdapterError> {
    let mut stmt = conn.prepare("PRAGMA main.table_list")?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, i64>(3)?,
            row.get::<_, i64>(4)?,
            row.get::<_, i64>(5)?,
        ))
    })?;
    let mut relations = Vec::new();
    let mut folded_names = BTreeSet::new();
    for row in rows {
        let (schema, name, relation_type, ncol, without_rowid, strict) = row?;
        if schema != MAIN_SCHEMA || name.starts_with("sqlite_") {
            continue;
        }
        if ncol < 0 {
            return Err(SqliteAdapterError::mapping(
                format!("relation {name}"),
                "PRAGMA table_list returned a negative column count",
            ));
        }
        if !folded_names.insert(fold_identifier(&name)) {
            return Err(SqliteAdapterError::mapping(
                format!("relation {name}"),
                "two relations collide under SQLite identifier comparison",
            ));
        }
        let kind = match relation_type.as_str() {
            "table" => RawRelationKind::Table(TableKind::BaseTable),
            "virtual" => RawRelationKind::Table(TableKind::Virtual),
            "shadow" => RawRelationKind::Table(TableKind::Shadow),
            "view" => RawRelationKind::View,
            unsupported => {
                return Err(SqliteAdapterError::mapping(
                    format!("relation {name}"),
                    format!("unsupported PRAGMA table_list relation type '{unsupported}'"),
                ));
            }
        };
        let schema_type = if kind == RawRelationKind::View {
            "view"
        } else {
            "table"
        };
        let sql = schema_entries
            .get(&(schema_type.to_owned(), name.clone()))
            .and_then(|entry| entry.sql.clone());
        if sql.is_none() && kind != RawRelationKind::Table(TableKind::Shadow) {
            return Err(SqliteAdapterError::mapping(
                format!("relation {name}"),
                "sqlite_schema did not provide a definition",
            ));
        }
        relations.push(RawRelation {
            name,
            kind,
            declared_column_count: u32::try_from(ncol).map_err(|_| {
                SqliteAdapterError::mapping("relation column count", "column count exceeds u32")
            })?,
            without_rowid: without_rowid != 0,
            strict: strict != 0,
            sql,
            columns: vec![],
            parsed_table: None,
        });
    }
    relations.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(relations)
}

fn read_relation_columns(
    conn: &Connection,
    relation_name: &str,
) -> Result<Vec<RawColumn>, SqliteAdapterError> {
    let mut stmt = conn.prepare(&format!(
        "PRAGMA main.table_xinfo({})",
        quote_string(relation_name)
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok(RawColumn {
            cid: row.get(0)?,
            name: row.get(1)?,
            data_type: row.get(2)?,
            not_null: row.get::<_, i64>(3)? != 0,
            default_value: row.get(4)?,
            primary_key_position: u32::try_from(row.get::<_, i64>(5)?).unwrap_or(u32::MAX),
            hidden: row.get(6)?,
        })
    })?;
    let mut columns = rows.collect::<rusqlite::Result<Vec<_>>>()?;
    columns.sort_by_key(|column| column.cid);
    for (ordinal, column) in columns.iter().enumerate() {
        if column.cid < 0 || column.cid as usize != ordinal {
            return Err(SqliteAdapterError::mapping(
                format!("column {}.{}", relation_name, column.name),
                "table_xinfo returned a non-contiguous column ordinal",
            ));
        }
        if column.primary_key_position == u32::MAX {
            return Err(SqliteAdapterError::mapping(
                format!("column {}.{}", relation_name, column.name),
                "primary-key ordinal exceeds the supported range",
            ));
        }
        if !matches!(column.hidden, 0..=3) {
            return Err(SqliteAdapterError::mapping(
                format!("column {}.{}", relation_name, column.name),
                format!("unknown table_xinfo hidden code {}", column.hidden),
            ));
        }
    }
    Ok(columns)
}

fn parse_relation_table(
    relation: &RawRelation,
) -> Result<Option<ParsedTableDefinition>, SqliteAdapterError> {
    let RawRelationKind::Table(kind) = relation.kind else {
        return Ok(None);
    };
    if kind == TableKind::Virtual || (kind == TableKind::Shadow && relation.sql.is_none()) {
        return Ok(None);
    }
    let sql = relation.sql.as_deref().ok_or_else(|| {
        SqliteAdapterError::mapping(
            format!("table {}", relation.name),
            "sqlite_schema did not provide CREATE TABLE SQL",
        )
    })?;
    let parsed = parse_table_definition(sql).map_err(|message| SqliteAdapterError::Parse {
        object: format!("table {}", relation.name),
        message,
    })?;
    if !same_identifier(&parsed.name, &relation.name) {
        return Err(SqliteAdapterError::mapping(
            format!("table {}", relation.name),
            format!(
                "parsed table name '{}' does not match catalog name",
                parsed.name
            ),
        ));
    }
    if parsed.strict != relation.strict || parsed.without_rowid != relation.without_rowid {
        return Err(SqliteAdapterError::mapping(
            format!("table {}", relation.name),
            "parsed STRICT/WITHOUT ROWID flags disagree with PRAGMA table_list",
        ));
    }
    Ok(Some(parsed))
}

fn read_indexes(
    conn: &Connection,
    table_name: &str,
    schema_entries: &BTreeMap<(String, String), RawSchemaEntry>,
) -> Result<Vec<RawIndex>, SqliteAdapterError> {
    let mut stmt = conn.prepare(&format!(
        "PRAGMA main.index_list({})",
        quote_string(table_name)
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, String>(1)?,
            row.get::<_, i64>(2)? != 0,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)? != 0,
        ))
    })?;
    let mut indexes = Vec::new();
    for row in rows {
        let (name, unique, origin, partial) = row?;
        if !matches!(origin.as_str(), "c" | "u" | "pk") {
            return Err(SqliteAdapterError::mapping(
                format!("index {name}"),
                format!("unknown PRAGMA index_list origin '{origin}'"),
            ));
        }
        let sql = schema_entries
            .get(&("index".to_owned(), name.clone()))
            .and_then(|entry| entry.sql.clone());
        let parsed = match sql.as_deref() {
            Some(sql) => {
                let parsed =
                    parse_index_definition(sql).map_err(|message| SqliteAdapterError::Parse {
                        object: format!("index {name}"),
                        message,
                    })?;
                if !same_identifier(&parsed.name, &name)
                    || !same_identifier(&parsed.table_name, table_name)
                    || parsed.unique != unique
                    || parsed.predicate.is_some() != partial
                {
                    return Err(SqliteAdapterError::mapping(
                        format!("index {name}"),
                        "parsed CREATE INDEX identity or flags disagree with PRAGMA index_list",
                    ));
                }
                Some(parsed)
            }
            None if origin == "c" => {
                return Err(SqliteAdapterError::mapping(
                    format!("index {name}"),
                    "explicit index has no sqlite_schema definition",
                ));
            }
            None => None,
        };
        let terms = read_index_terms(conn, &name)?;
        let key_term_count = terms.iter().filter(|term| term.key).count();
        if parsed
            .as_ref()
            .is_some_and(|definition| definition.terms.len() != key_term_count)
        {
            return Err(SqliteAdapterError::mapping(
                format!("index {name}"),
                format!(
                    "CREATE INDEX has {} key term(s), but index_xinfo returned {key_term_count}",
                    parsed.as_ref().map_or(0, |value| value.terms.len())
                ),
            ));
        }
        indexes.push(RawIndex {
            table_name: table_name.to_owned(),
            name,
            unique,
            origin,
            partial,
            sql,
            parsed,
            terms,
        });
    }
    indexes.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(indexes)
}

fn read_index_terms(
    conn: &Connection,
    index_name: &str,
) -> Result<Vec<RawIndexTerm>, SqliteAdapterError> {
    let mut stmt = conn.prepare(&format!(
        "PRAGMA main.index_xinfo({})",
        quote_string(index_name)
    ))?;
    let rows = stmt.query_map([], |row| {
        let sequence = row.get::<_, i64>(0)?;
        Ok((
            sequence,
            row.get::<_, i64>(1)?,
            row.get::<_, Option<String>>(2)?,
            row.get::<_, i64>(3)? != 0,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, i64>(5)? != 0,
        ))
    })?;
    let mut terms = Vec::new();
    for row in rows {
        let (sequence, cid, column_name, descending, collation, key) = row?;
        if sequence < 0 {
            return Err(SqliteAdapterError::mapping(
                format!("index {index_name}"),
                "index_xinfo returned a negative term sequence",
            ));
        }
        if cid >= 0 && column_name.is_none() {
            return Err(SqliteAdapterError::mapping(
                format!("index {index_name}"),
                "index_xinfo omitted the name of a direct column term",
            ));
        }
        if cid < -2 {
            return Err(SqliteAdapterError::mapping(
                format!("index {index_name}"),
                format!("index_xinfo returned unknown column code {cid}"),
            ));
        }
        terms.push(RawIndexTerm {
            sequence: u32::try_from(sequence).map_err(|_| {
                SqliteAdapterError::mapping(
                    format!("index {index_name}"),
                    "index term sequence exceeds u32",
                )
            })?,
            cid,
            column_name,
            descending,
            collation,
            key,
        });
    }
    terms.sort_by_key(|term| term.sequence);
    for (expected, term) in terms.iter().enumerate() {
        if term.sequence as usize != expected {
            return Err(SqliteAdapterError::mapping(
                format!("index {index_name}"),
                "index_xinfo returned non-contiguous term sequences",
            ));
        }
    }
    Ok(terms)
}

fn read_foreign_keys(
    conn: &Connection,
    table_name: &str,
) -> Result<Vec<RawForeignKey>, SqliteAdapterError> {
    let mut stmt = conn.prepare(&format!(
        "PRAGMA main.foreign_key_list({})",
        quote_string(table_name)
    ))?;
    let rows = stmt.query_map([], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, String>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, Option<String>>(4)?,
            row.get::<_, String>(5)?,
            row.get::<_, String>(6)?,
            row.get::<_, String>(7)?,
        ))
    })?;
    let mut grouped = BTreeMap::<i64, RawForeignKey>::new();
    for row in rows {
        let (
            id,
            sequence,
            referenced_table,
            source_column,
            referenced_column,
            on_update,
            on_delete,
            match_name,
        ) = row?;
        if id < 0 || sequence < 0 {
            return Err(SqliteAdapterError::mapping(
                format!("foreign key on {table_name}"),
                "foreign_key_list returned a negative id or sequence",
            ));
        }
        let entry = grouped.entry(id).or_insert_with(|| RawForeignKey {
            id,
            parts: vec![],
            on_update: normalize_pragma_token(&on_update),
            on_delete: normalize_pragma_token(&on_delete),
            match_name: normalize_pragma_token(&match_name),
        });
        if entry.on_update != normalize_pragma_token(&on_update)
            || entry.on_delete != normalize_pragma_token(&on_delete)
            || entry.match_name != normalize_pragma_token(&match_name)
        {
            return Err(SqliteAdapterError::mapping(
                format!("foreign key {table_name}.{id}"),
                "foreign_key_list returned inconsistent actions for one composite key",
            ));
        }
        entry.parts.push(RawForeignKeyPart {
            sequence: u32::try_from(sequence).map_err(|_| {
                SqliteAdapterError::mapping(
                    format!("foreign key {table_name}.{id}"),
                    "foreign-key sequence exceeds u32",
                )
            })?,
            referenced_table,
            source_column,
            referenced_column: referenced_column.filter(|name| !name.is_empty()),
        });
    }
    let mut foreign_keys = grouped.into_values().collect::<Vec<_>>();
    foreign_keys.sort_by_key(|foreign_key| foreign_key.id);
    for foreign_key in &mut foreign_keys {
        foreign_key.parts.sort_by_key(|part| part.sequence);
        for (expected, part) in foreign_key.parts.iter().enumerate() {
            if part.sequence as usize != expected {
                return Err(SqliteAdapterError::mapping(
                    format!("foreign key {table_name}.{}", foreign_key.id),
                    "foreign_key_list returned non-contiguous column sequences",
                ));
            }
        }
    }
    Ok(foreign_keys)
}

fn read_triggers(
    schema_entries: &BTreeMap<(String, String), RawSchemaEntry>,
) -> Result<Vec<RawTrigger>, SqliteAdapterError> {
    let mut triggers = Vec::new();
    for entry in schema_entries
        .values()
        .filter(|entry| entry.object_type == "trigger" && !entry.name.starts_with("sqlite_"))
    {
        let sql = entry.sql.clone().ok_or_else(|| {
            SqliteAdapterError::mapping(
                format!("trigger {}", entry.name),
                "sqlite_schema did not provide CREATE TRIGGER SQL",
            )
        })?;
        let parsed =
            parse_trigger_definition(&sql).map_err(|message| SqliteAdapterError::Parse {
                object: format!("trigger {}", entry.name),
                message,
            })?;
        if !same_identifier(&parsed.name, &entry.name)
            || !same_identifier(&parsed.owner_name, &entry.owner_name)
        {
            return Err(SqliteAdapterError::mapping(
                format!("trigger {}", entry.name),
                "parsed trigger identity disagrees with sqlite_schema",
            ));
        }
        triggers.push(RawTrigger {
            name: entry.name.clone(),
            owner_name: entry.owner_name.clone(),
            sql,
            parsed,
        });
    }
    triggers.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(triggers)
}

fn normalize_pragma_token(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(' ', "_")
}

struct SqliteSnapshotMapper<'connection> {
    conn: &'connection Connection,
    snapshot_source_kind: &'connection str,
    object_source_kind: &'connection str,
    connection_alias: &'connection str,
    notes: Vec<String>,
}

impl<'connection> SqliteSnapshotMapper<'connection> {
    fn new(
        conn: &'connection Connection,
        snapshot_source_kind: &'connection str,
        object_source_kind: &'connection str,
        connection_alias: &'connection str,
        notes: Vec<String>,
    ) -> Self {
        Self {
            conn,
            snapshot_source_kind,
            object_source_kind,
            connection_alias,
            notes,
        }
    }

    fn map(self, raw: RawSqliteCatalog) -> Result<CatalogDiscovery, SqliteAdapterError> {
        let database_key = self.key(ObjectKind::Database, MAIN_CATALOG, None);
        let schema_key = self.key(ObjectKind::Schema, MAIN_SCHEMA, None);
        let database = DatabaseObject {
            key: database_key.clone(),
            name: MAIN_CATALOG.to_owned(),
        };
        let schemas = vec![SchemaObject {
            key: schema_key.clone(),
            database_key,
            name: MAIN_SCHEMA.to_owned(),
        }];

        let mut tables = Vec::new();
        let mut table_keys = BTreeMap::new();
        let mut view_keys = BTreeMap::new();
        for relation in &raw.relations {
            match relation.kind {
                RawRelationKind::Table(kind) => {
                    let table_key = self.key(ObjectKind::Table, &relation.name, None);
                    table_keys.insert(fold_identifier(&relation.name), table_key.clone());
                    tables.push(TableObject {
                        key: table_key,
                        schema_key: schema_key.clone(),
                        name: relation.name.clone(),
                        kind,
                    });
                }
                RawRelationKind::View => {
                    view_keys.insert(
                        fold_identifier(&relation.name),
                        self.key(ObjectKind::View, &relation.name, None),
                    );
                }
            }
        }

        let mut columns = Vec::new();
        let mut metadata = CanonicalMetadata::default();
        let mut relation_column_keys = BTreeMap::<(String, String), ObjectKey>::new();
        let mut primary_key_columns = BTreeMap::<String, Vec<ObjectKey>>::new();
        for relation in &raw.relations {
            match relation.kind {
                RawRelationKind::Table(_) => {
                    let table_key = lookup_key(&table_keys, &relation.name, "table")?;
                    let parsed_columns = relation
                        .parsed_table
                        .as_ref()
                        .map(|table| {
                            table
                                .columns
                                .iter()
                                .map(|column| (fold_identifier(&column.name), column))
                                .collect::<BTreeMap<_, _>>()
                        })
                        .unwrap_or_default();
                    let pk_count = relation
                        .columns
                        .iter()
                        .filter(|column| column.primary_key_position > 0)
                        .count();
                    let mut ordered_primary_key = Vec::new();
                    for raw_column in &relation.columns {
                        let column_key = self.key(
                            ObjectKind::Column,
                            &relation.name,
                            Some(raw_column.name.clone()),
                        );
                        let parsed_column = parsed_columns.get(&fold_identifier(&raw_column.name));
                        let generated = matches!(raw_column.hidden, 2 | 3);
                        if generated
                            && parsed_column
                                .and_then(|column| column.generated_expression.as_ref())
                                .is_none()
                        {
                            return Err(SqliteAdapterError::mapping(
                                format!("column {}.{}", relation.name, raw_column.name),
                                "table_xinfo marks the column generated, but CREATE TABLE has no generation expression",
                            ));
                        }
                        let integer_primary_key_alias = pk_count == 1
                            && raw_column.primary_key_position == 1
                            && raw_column.data_type.eq_ignore_ascii_case("INTEGER")
                            && !relation.without_rowid;
                        let primary_key_is_not_null = raw_column.primary_key_position > 0
                            && (relation.without_rowid
                                || relation.strict
                                || integer_primary_key_alias);
                        columns.push(ColumnObject {
                            key: column_key.clone(),
                            table_key: table_key.clone(),
                            name: raw_column.name.clone(),
                            ordinal_position: u32::try_from(raw_column.cid)
                                .map_err(|_| {
                                    SqliteAdapterError::mapping(
                                        format!("column {}.{}", relation.name, raw_column.name),
                                        "column ordinal exceeds u32",
                                    )
                                })?
                                .saturating_add(1),
                            data_type: raw_column.data_type.clone(),
                            is_nullable: !raw_column.not_null && !primary_key_is_not_null,
                            default_value: raw_column.default_value.clone(),
                            is_generated: generated,
                        });
                        relation_column_keys.insert(
                            (
                                fold_identifier(&relation.name),
                                fold_identifier(&raw_column.name),
                            ),
                            column_key.clone(),
                        );
                        if raw_column.primary_key_position > 0 {
                            ordered_primary_key
                                .push((raw_column.primary_key_position, column_key.clone()));
                        }
                        let mut properties = BTreeMap::new();
                        properties.insert(
                            "declared_not_null".to_owned(),
                            MetadataValue::Boolean(raw_column.not_null),
                        );
                        properties.insert(
                            "sqlite_hidden_code".to_owned(),
                            MetadataValue::Integer(raw_column.hidden),
                        );
                        if let Some(parsed_column) = parsed_column {
                            if let Some(storage) = &parsed_column.generated_storage {
                                properties.insert(
                                    "generated_storage".to_owned(),
                                    MetadataValue::String(storage.clone()),
                                );
                            }
                            if let Some(collation) = &parsed_column.collation {
                                properties.insert(
                                    "collation".to_owned(),
                                    MetadataValue::String(collation.clone()),
                                );
                            }
                        }
                        metadata.annotations.push(ObjectAnnotation {
                            object_key: column_key,
                            definition: parsed_column
                                .and_then(|column| column.generated_expression.clone()),
                            properties,
                        });
                    }
                    ordered_primary_key.sort_by_key(|(position, _)| *position);
                    primary_key_columns.insert(
                        fold_identifier(&relation.name),
                        ordered_primary_key
                            .into_iter()
                            .map(|(_, key)| key)
                            .collect(),
                    );
                    metadata
                        .annotations
                        .push(table_annotation(table_key, relation));
                }
                RawRelationKind::View => {
                    let view_key = lookup_key(&view_keys, &relation.name, "view")?;
                    for raw_column in &relation.columns {
                        let column_key = self.key(
                            ObjectKind::ViewColumn,
                            &relation.name,
                            Some(raw_column.name.clone()),
                        );
                        relation_column_keys.insert(
                            (
                                fold_identifier(&relation.name),
                                fold_identifier(&raw_column.name),
                            ),
                            column_key.clone(),
                        );
                        let mut properties = BTreeMap::new();
                        properties.insert(
                            "ordinal_position".to_owned(),
                            MetadataValue::Unsigned(
                                u64::try_from(raw_column.cid).unwrap_or_default() + 1,
                            ),
                        );
                        properties.insert(
                            "data_type".to_owned(),
                            MetadataValue::String(raw_column.data_type.clone()),
                        );
                        properties.insert(
                            "nullable".to_owned(),
                            MetadataValue::Boolean(!raw_column.not_null),
                        );
                        metadata.objects.push(MetadataObject {
                            key: column_key,
                            parent_key: Some(view_key.clone()),
                            name: raw_column.name.clone(),
                            extension_kind: None,
                            definition: None,
                            properties,
                        });
                    }
                }
            }
        }

        let mut constraints = Vec::new();
        let mut check_dependency_count = 0_u64;
        self.map_generated_dependencies(
            &raw.relations,
            &table_keys,
            &relation_column_keys,
            &mut metadata,
        )?;
        for relation in raw
            .relations
            .iter()
            .filter(|relation| relation.kind.is_table())
        {
            let mapped = self.map_constraints(
                relation,
                raw.foreign_keys
                    .get(&relation.name)
                    .map(Vec::as_slice)
                    .unwrap_or_default(),
                &raw.indexes,
                &table_keys,
                &relation_column_keys,
                &primary_key_columns,
                &mut metadata,
            )?;
            check_dependency_count += mapped.check_dependency_count;
            constraints.extend(mapped.constraints);
        }

        let mapped_indexes = self.map_indexes(
            &raw.indexes,
            &table_keys,
            &relation_column_keys,
            &mut metadata,
        )?;
        let raw_direct_index_columns = raw
            .indexes
            .iter()
            .flat_map(|index| index.terms.iter())
            .filter(|term| term.key && term.cid >= 0)
            .count() as u64;
        if mapped_indexes.direct_column_count != raw_direct_index_columns {
            return Err(SqliteAdapterError::mapping(
                "index column reconciliation",
                format!(
                    "index_xinfo discovered {raw_direct_index_columns} direct key column(s), but mapper emitted {}",
                    mapped_indexes.direct_column_count
                ),
            ));
        }
        let indexes = mapped_indexes.indexes;

        let mut views = Vec::new();
        let mut view_dependency_count = 0_u64;
        for relation in raw
            .relations
            .iter()
            .filter(|relation| relation.kind == RawRelationKind::View)
        {
            let view_key = lookup_key(&view_keys, &relation.name, "view")?;
            let discovered_dependencies =
                self.view_dependencies(relation, &table_keys, &view_keys, &relation_column_keys)?;
            let mut dependencies = Vec::new();
            for dependency in discovered_dependencies {
                if dependency.object_kind == ObjectKind::ViewColumn {
                    metadata.relationships.push(MetadataRelationship {
                        kind: MetadataRelationshipKind::DependsOn,
                        from_key: view_key.clone(),
                        to_key: dependency,
                        ordinal: None,
                        properties: BTreeMap::new(),
                    });
                } else {
                    dependencies.push(dependency);
                }
            }
            view_dependency_count += dependencies.len() as u64;
            views.push(ViewObject {
                key: view_key,
                schema_key: schema_key.clone(),
                name: relation.name.clone(),
                definition: relation.sql.clone(),
                depends_on: dependencies,
            });
        }

        let triggers = self.map_triggers(
            &raw.triggers,
            &raw.relations,
            &table_keys,
            &view_keys,
            &relation_column_keys,
            &mut metadata,
        )?;
        deduplicate_metadata_relationships(&mut metadata.relationships);

        let schema = SchemaSnapshot {
            source_kind: self.snapshot_source_kind.to_owned(),
            connection_alias: self.connection_alias.to_owned(),
            database,
            schemas,
            tables,
            columns,
            constraints,
            indexes,
            views,
            triggers,
            routines: vec![],
            capabilities: AdapterCapabilities {
                source_kind: self.snapshot_source_kind.to_owned(),
                metadata_only: true,
                schemas: true,
                tables: true,
                columns: true,
                constraints: true,
                indexes: true,
                views: CapabilitySupport::Supported,
                triggers: CapabilitySupport::Supported,
                routines: CapabilitySupport::Supported,
                dependencies: CapabilitySupport::Supported,
                limitations: vec![],
                notes: self.notes,
            },
        };
        let discovered_counts = discovery_counts(
            &raw,
            check_dependency_count,
            view_dependency_count,
            metadata.relationships.len() as u64,
        );
        let capability_checks = vec![
            CapabilityCheck {
                name: "catalog_scope".to_owned(),
                evidence: format!(
                    "PRAGMA database_list returned [{}]; selected main schema at schema_version {}",
                    raw.database_names.join(", "),
                    raw.schema_version
                ),
            },
            CapabilityCheck {
                name: "metadata_only".to_owned(),
                evidence: "Inventory came from sqlite_schema/table_list/table_xinfo/index_list/index_xinfo/foreign_key_list; dependency probes only prepared EXPLAIN statements and never stepped them".to_owned(),
            },
            CapabilityCheck {
                name: "routines_absent".to_owned(),
                evidence: "SQLite has no persisted schema routine catalog; connection-local functions are outside the selected database schema".to_owned(),
            },
            CapabilityCheck {
                name: "sql_grammar".to_owned(),
                evidence: "Every persisted CREATE TABLE, CREATE INDEX, and CREATE TRIGGER definition requiring semantic mapping was parsed with sqlite3-parser 0.17.0".to_owned(),
            },
            CapabilityCheck {
                name: "system_scope".to_owned(),
                evidence: "SQLite-owned names beginning with sqlite_ were excluded consistently from inventory and count probes".to_owned(),
            },
        ];

        Ok(CatalogDiscovery {
            snapshot: CanonicalSchemaSnapshot { schema, metadata },
            adapter: AdapterIdentity {
                name: "database-memory-sqlite".to_owned(),
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            server: ServerIdentity {
                product: "SQLite".to_owned(),
                version: raw.sqlite_version,
            },
            scope: IntrospectionScope {
                catalogs: vec![MAIN_CATALOG.to_owned()],
                schemas: vec![MAIN_SCHEMA.to_owned()],
            },
            discovered_counts,
            capability_checks,
        })
    }

    fn key(
        &self,
        object_kind: ObjectKind,
        object_name: &str,
        sub_object: Option<String>,
    ) -> ObjectKey {
        ObjectKey::new(
            self.object_source_kind,
            self.connection_alias,
            MAIN_CATALOG,
            MAIN_SCHEMA,
            object_kind,
            object_name,
            sub_object,
        )
    }
}

