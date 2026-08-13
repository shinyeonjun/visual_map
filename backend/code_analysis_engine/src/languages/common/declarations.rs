use crate::facts::CodeUnitKind;

/// Tree-sitter 선언 노드를 공통 코드 유닛 종류로 변환한다.
pub(super) fn declaration_kind(
    node_kind: &str,
    parent_kind: Option<&CodeUnitKind>,
) -> Option<CodeUnitKind> {
    let kind = match node_kind {
        "function_declaration"
        | "function_definition"
        | "function_item"
        | "function_definition_statement"
        | "method_declaration"
        | "method_definition"
        | "method_elem"
        | "function_signature_item" => {
            if parent_kind.is_some_and(is_type_container) {
                CodeUnitKind::Method
            } else {
                CodeUnitKind::Function
            }
        }
        "method_signature" | "abstract_method_signature" => {
            if parent_kind.is_some_and(is_type_container) {
                CodeUnitKind::Method
            } else {
                return None;
            }
        }
        "constructor_declaration" | "constructor" => CodeUnitKind::Constructor,
        "constructor_signature"
        | "constant_constructor_signature"
        | "factory_constructor_signature"
        | "redirecting_factory_constructor_signature" => {
            if parent_kind.is_some_and(is_type_container) {
                CodeUnitKind::Constructor
            } else {
                return None;
            }
        }
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
        "namespace_definition" | "namespace_declaration" | "file_scoped_namespace_declaration" => {
            CodeUnitKind::Namespace
        }
        "package_declaration" => CodeUnitKind::Package,
        "module_declaration" | "internal_module" | "mod_item" => CodeUnitKind::Module,
        "type_declaration" => CodeUnitKind::Record,
        "property_declaration"
        | "property_definition"
        | "field_definition"
        | "public_field_definition"
        | "property_signature"
        | "getter_declaration"
        | "setter_declaration"
        | "external_getter_declaration"
        | "external_setter_declaration" => CodeUnitKind::Property,
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
