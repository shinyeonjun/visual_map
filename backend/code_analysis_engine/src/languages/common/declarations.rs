use crate::facts::{CodeUnitKind, FactBundle};

/// Tree-sitter 선언 노드를 공통 코드 유닛 종류로 변환한다.
pub(super) fn declaration_kind(
    node_kind: &str,
    current_parent: &str,
    bundle: &FactBundle,
) -> Option<CodeUnitKind> {
    let kind = match node_kind {
        "function_declaration"
        | "function_definition"
        | "function_item"
        | "function_definition_statement"
        | "method_declaration"
        | "method_definition"
        | "method_signature"
        | "function_signature_item"
        | "function_signature" => {
            if bundle
                .units
                .iter()
                .any(|unit| unit.id == current_parent && is_type_container(&unit.kind))
            {
                CodeUnitKind::Method
            } else {
                CodeUnitKind::Function
            }
        }
        "constructor_declaration" | "constructor" => CodeUnitKind::Constructor,
        "lambda_expression"
        | "arrow_function"
        | "function_expression"
        | "lambda"
        | "func_literal"
        | "closure_expression"
        | "closure"
        | "function_literal" => CodeUnitKind::Lambda,
        "class_declaration"
        | "class_definition"
        | "class_specifier"
        | "class_definition_statement"
        | "abstract_class_declaration" => CodeUnitKind::Class,
        "interface_declaration" | "interface_definition" | "annotation_type_declaration" => {
            CodeUnitKind::Interface
        }
        "struct_specifier" | "struct_item" | "struct_declaration" => CodeUnitKind::Struct,
        "enum_declaration" | "enum_specifier" | "enum_item" => CodeUnitKind::Enum,
        "trait_item" | "trait_declaration" => CodeUnitKind::Trait,
        "impl_item" | "impl_block" => CodeUnitKind::Impl,
        "record_declaration" | "record_definition" => CodeUnitKind::Record,
        "mixin_declaration" | "mixin" => CodeUnitKind::Mixin,
        "extension_declaration" | "extension_declaration_statement" => CodeUnitKind::Extension,
        "namespace_definition" | "namespace_declaration" => CodeUnitKind::Namespace,
        "package_declaration" => CodeUnitKind::Package,
        "module_declaration" | "internal_module" | "mod_item" => CodeUnitKind::Module,
        "type_declaration" => CodeUnitKind::Record,
        "property_declaration" | "property_definition" => CodeUnitKind::Property,
        "type_alias_declaration" | "type_alias" | "type_item" => CodeUnitKind::TypeAlias,
        _ => return None,
    };
    Some(kind)
}

pub(super) fn is_type_container(kind: &CodeUnitKind) -> bool {
    matches!(
        kind,
        CodeUnitKind::Class
            | CodeUnitKind::Interface
            | CodeUnitKind::Struct
            | CodeUnitKind::Enum
            | CodeUnitKind::Trait
            | CodeUnitKind::Impl
            | CodeUnitKind::Record
            | CodeUnitKind::Mixin
            | CodeUnitKind::Extension
    )
}
