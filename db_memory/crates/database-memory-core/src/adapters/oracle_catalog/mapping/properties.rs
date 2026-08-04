fn constraint_properties(constraint: &RawConstraint) -> BTreeMap<String, MetadataValue> {
    let mut properties = BTreeMap::new();
    insert_string(&mut properties, "status", &constraint.status);
    insert_string(&mut properties, "deferrable", &constraint.deferrable);
    insert_string(&mut properties, "deferred", &constraint.deferred);
    insert_string(&mut properties, "validated", &constraint.validated);
    insert_string(&mut properties, "generated", &constraint.generated);
    insert_optional_string(
        &mut properties,
        "delete_rule",
        constraint.delete_rule.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "index_owner",
        constraint.index_owner.as_deref(),
    );
    insert_optional_string(
        &mut properties,
        "index_name",
        constraint.index_name.as_deref(),
    );
    insert_optional_string(&mut properties, "invalid", constraint.invalid.as_deref());
    insert_optional_string(
        &mut properties,
        "view_related",
        constraint.view_related.as_deref(),
    );
    properties
}

fn format_oracle_data_type(column: &RawColumn) -> String {
    let type_name = column
        .data_type_owner
        .as_deref()
        .map(|owner| format!("{owner}.{}", column.data_type))
        .unwrap_or_else(|| column.data_type.clone());
    match column.data_type.as_str() {
        "NUMBER" => match (column.data_precision, column.data_scale) {
            (Some(precision), Some(scale)) => format!("{type_name}({precision},{scale})"),
            (Some(precision), None) => format!("{type_name}({precision})"),
            _ => type_name,
        },
        "FLOAT" => column
            .data_precision
            .map(|precision| format!("{type_name}({precision})"))
            .unwrap_or(type_name),
        "CHAR" | "VARCHAR2" | "NCHAR" | "NVARCHAR2" => {
            let unit = match column.char_used.as_deref() {
                Some("C") => " CHAR",
                Some("B") => " BYTE",
                _ => "",
            };
            format!(
                "{type_name}({}{unit})",
                column.char_length.unwrap_or(column.data_length)
            )
        }
        "RAW" | "UROWID" => format!("{type_name}({})", column.data_length),
        _ => type_name,
    }
}
