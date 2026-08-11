//! Conservative execution-dispatch classification.
//!
//! A semantic provider resolves a written call token to a definition, while
//! the source language decides whether that definition is necessarily the
//! runtime target. TracePath keeps resolved non-direct edges only as explicit
//! candidate gaps, so this module deliberately prefers a typed non-direct
//! result over a convenient direct guess.

use super::{
    BTreeMap, DefinitionDraft, DispatchKind, FactNodeKind, LanguageRelationKind,
    ProgrammingLanguage, ProviderSymbolId, Visibility,
};
use crate::{CallSiteForm, SyntaxCallSite};

pub(super) fn classify_execution_dispatch(
    language: ProgrammingLanguage,
    relation_kind: LanguageRelationKind,
    site: &SyntaxCallSite,
    target: &DefinitionDraft,
    definitions: &BTreeMap<ProviderSymbolId, DefinitionDraft>,
) -> DispatchKind {
    let owner_kind = target
        .parent_symbol_id
        .as_ref()
        .and_then(|owner| definitions.get(owner))
        .map(|owner| owner.canonical_kind_hint);
    classify_target(
        language,
        relation_kind,
        site.form,
        DispatchTarget {
            kind: target.canonical_kind_hint,
            signature: target.signature.as_deref(),
            visibility: target.visibility,
            owner_kind,
        },
    )
}

#[derive(Clone, Copy)]
struct DispatchTarget<'a> {
    kind: FactNodeKind,
    signature: Option<&'a str>,
    visibility: Visibility,
    owner_kind: Option<FactNodeKind>,
}

fn classify_target(
    language: ProgrammingLanguage,
    relation_kind: LanguageRelationKind,
    form: CallSiteForm,
    target: DispatchTarget<'_>,
) -> DispatchKind {
    if relation_kind == LanguageRelationKind::Constructs || form == CallSiteForm::Construct {
        return constructor_dispatch(language, target);
    }

    match language {
        // These languages permit runtime rebinding/metaclass/proxy behavior.
        // The resolved source definition is useful, but is not an exact
        // runtime-target proof.
        ProgrammingLanguage::TypeScript
        | ProgrammingLanguage::JavaScript
        | ProgrammingLanguage::Python => DispatchKind::Dynamic,
        ProgrammingLanguage::Java => java_dispatch(target),
        ProgrammingLanguage::CSharp => csharp_dispatch(target),
        ProgrammingLanguage::C => {
            if target.kind == FactNodeKind::Function {
                DispatchKind::Direct
            } else {
                DispatchKind::Dynamic
            }
        }
        ProgrammingLanguage::Cpp => cpp_dispatch(target),
        ProgrammingLanguage::Go => go_dispatch(target),
        ProgrammingLanguage::Rust => rust_dispatch(target),
        ProgrammingLanguage::Dart => dart_dispatch(target),
    }
}

fn constructor_dispatch(language: ProgrammingLanguage, target: DispatchTarget<'_>) -> DispatchKind {
    match language {
        ProgrammingLanguage::TypeScript
        | ProgrammingLanguage::JavaScript
        | ProgrammingLanguage::Python => DispatchKind::Dynamic,
        ProgrammingLanguage::Java | ProgrammingLanguage::CSharp | ProgrammingLanguage::Cpp
            if target.kind == FactNodeKind::Constructor =>
        {
            DispatchKind::Direct
        }
        ProgrammingLanguage::Java | ProgrammingLanguage::CSharp | ProgrammingLanguage::Cpp => {
            DispatchKind::Unknown
        }
        ProgrammingLanguage::Dart if has_modifier(target.signature, "factory") => {
            DispatchKind::Dynamic
        }
        ProgrammingLanguage::Dart if target.kind == FactNodeKind::Constructor => {
            DispatchKind::Direct
        }
        // A class-only Dart target does not prove whether the selected
        // constructor is a redirecting factory.
        ProgrammingLanguage::Dart => DispatchKind::Dynamic,
        ProgrammingLanguage::C | ProgrammingLanguage::Go | ProgrammingLanguage::Rust => {
            if matches!(
                target.kind,
                FactNodeKind::Constructor | FactNodeKind::Function
            ) {
                DispatchKind::Direct
            } else {
                DispatchKind::Unknown
            }
        }
    }
}

fn java_dispatch(target: DispatchTarget<'_>) -> DispatchKind {
    if target.kind == FactNodeKind::Constructor
        || target.kind == FactNodeKind::Function
        || target.visibility == Visibility::Private
        || has_any_modifier(target.signature, &["static", "private", "final"])
    {
        DispatchKind::Direct
    } else if target.owner_kind == Some(FactNodeKind::Interface) {
        DispatchKind::Interface
    } else if target.kind == FactNodeKind::Method {
        DispatchKind::Virtual
    } else {
        DispatchKind::Unknown
    }
}

fn csharp_dispatch(target: DispatchTarget<'_>) -> DispatchKind {
    if target.kind == FactNodeKind::Constructor
        || target.kind == FactNodeKind::Function
        || target.visibility == Visibility::Private
        || has_any_modifier(target.signature, &["static", "private", "sealed"])
    {
        DispatchKind::Direct
    } else if target.owner_kind == Some(FactNodeKind::Interface) {
        DispatchKind::Interface
    } else if has_any_modifier(target.signature, &["virtual", "abstract", "override"]) {
        DispatchKind::Virtual
    } else if target.kind == FactNodeKind::Method {
        // C# instance methods are non-virtual unless explicitly declared
        // virtual/abstract/override.
        DispatchKind::Direct
    } else {
        DispatchKind::Unknown
    }
}

fn cpp_dispatch(target: DispatchTarget<'_>) -> DispatchKind {
    if target.kind == FactNodeKind::Constructor || target.kind == FactNodeKind::Function {
        DispatchKind::Direct
    } else if has_any_modifier(target.signature, &["virtual", "override"])
        || target.signature.is_some_and(is_cpp_pure_virtual)
    {
        DispatchKind::Virtual
    } else if target.kind == FactNodeKind::Method {
        DispatchKind::Direct
    } else {
        DispatchKind::Unknown
    }
}

fn go_dispatch(target: DispatchTarget<'_>) -> DispatchKind {
    if target.kind == FactNodeKind::Function {
        DispatchKind::Direct
    } else if target.owner_kind == Some(FactNodeKind::Interface) {
        DispatchKind::Interface
    } else if target.kind == FactNodeKind::Method
        && matches!(
            target.owner_kind,
            Some(FactNodeKind::Struct | FactNodeKind::Type)
        )
    {
        DispatchKind::Direct
    } else {
        DispatchKind::Unknown
    }
}

fn rust_dispatch(target: DispatchTarget<'_>) -> DispatchKind {
    if target.kind == FactNodeKind::Function || target.kind == FactNodeKind::Constructor {
        DispatchKind::Direct
    } else if target.owner_kind == Some(FactNodeKind::Trait) {
        DispatchKind::Interface
    } else if target.kind == FactNodeKind::Method
        && matches!(
            target.owner_kind,
            Some(FactNodeKind::Struct | FactNodeKind::Enum | FactNodeKind::Type)
        )
    {
        DispatchKind::Direct
    } else {
        DispatchKind::Unknown
    }
}

fn dart_dispatch(target: DispatchTarget<'_>) -> DispatchKind {
    if target.kind == FactNodeKind::Function
        || target.kind == FactNodeKind::Constructor
        || has_modifier(target.signature, "static")
    {
        DispatchKind::Direct
    } else if matches!(
        target.owner_kind,
        Some(FactNodeKind::Interface | FactNodeKind::Trait)
    ) {
        DispatchKind::Interface
    } else if target.kind == FactNodeKind::Method {
        DispatchKind::Virtual
    } else {
        DispatchKind::Unknown
    }
}

fn is_cpp_pure_virtual(signature: &str) -> bool {
    signature
        .split_whitespace()
        .collect::<String>()
        .ends_with("=0")
}

fn has_any_modifier(signature: Option<&str>, expected: &[&str]) -> bool {
    expected
        .iter()
        .any(|modifier| has_modifier(signature, modifier))
}

fn has_modifier(signature: Option<&str>, expected: &str) -> bool {
    signature.is_some_and(|signature| {
        signature
            .split(|character: char| !(character == '_' || character.is_alphanumeric()))
            .any(|token| token == expected)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(
        kind: FactNodeKind,
        signature: Option<&str>,
        visibility: Visibility,
        owner_kind: Option<FactNodeKind>,
    ) -> DispatchTarget<'_> {
        DispatchTarget {
            kind,
            signature,
            visibility,
            owner_kind,
        }
    }

    #[test]
    fn only_language_proven_direct_calls_receive_direct_dispatch() {
        let free = target(
            FactNodeKind::Function,
            Some("void Target()"),
            Visibility::Public,
            None,
        );
        for language in [
            ProgrammingLanguage::C,
            ProgrammingLanguage::Cpp,
            ProgrammingLanguage::Go,
            ProgrammingLanguage::Rust,
            ProgrammingLanguage::Dart,
        ] {
            assert_eq!(
                classify_target(
                    language,
                    LanguageRelationKind::Calls,
                    CallSiteForm::Call,
                    free
                ),
                DispatchKind::Direct,
                "{}",
                language.as_str()
            );
        }
        for language in [
            ProgrammingLanguage::TypeScript,
            ProgrammingLanguage::JavaScript,
            ProgrammingLanguage::Python,
        ] {
            assert_eq!(
                classify_target(
                    language,
                    LanguageRelationKind::Calls,
                    CallSiteForm::Call,
                    free
                ),
                DispatchKind::Dynamic,
                "{}",
                language.as_str()
            );
        }
    }

    #[test]
    fn java_and_csharp_follow_their_different_default_method_dispatch_rules() {
        let ordinary_method = target(
            FactNodeKind::Method,
            Some("public void Target()"),
            Visibility::Public,
            Some(FactNodeKind::Class),
        );
        assert_eq!(java_dispatch(ordinary_method), DispatchKind::Virtual);
        assert_eq!(csharp_dispatch(ordinary_method), DispatchKind::Direct);

        let interface_method = target(
            FactNodeKind::Method,
            Some("void Target()"),
            Visibility::Public,
            Some(FactNodeKind::Interface),
        );
        assert_eq!(java_dispatch(interface_method), DispatchKind::Interface);
        assert_eq!(csharp_dispatch(interface_method), DispatchKind::Interface);
    }

    #[test]
    fn static_private_and_non_virtual_compiled_calls_are_direct() {
        let static_method = target(
            FactNodeKind::Method,
            Some("public static void Target()"),
            Visibility::Public,
            Some(FactNodeKind::Class),
        );
        assert_eq!(java_dispatch(static_method), DispatchKind::Direct);
        assert_eq!(csharp_dispatch(static_method), DispatchKind::Direct);

        let cpp_virtual = target(
            FactNodeKind::Method,
            Some("virtual void Target() = 0"),
            Visibility::Public,
            Some(FactNodeKind::Class),
        );
        assert_eq!(cpp_dispatch(cpp_virtual), DispatchKind::Virtual);
    }

    #[test]
    fn constructors_stay_dynamic_where_runtime_rebinding_is_legal() {
        let constructor = target(
            FactNodeKind::Constructor,
            Some("Target()"),
            Visibility::Public,
            Some(FactNodeKind::Class),
        );
        assert_eq!(
            constructor_dispatch(ProgrammingLanguage::Java, constructor),
            DispatchKind::Direct
        );
        assert_eq!(
            constructor_dispatch(ProgrammingLanguage::JavaScript, constructor),
            DispatchKind::Dynamic
        );
        let factory = target(
            FactNodeKind::Constructor,
            Some("factory Target()"),
            Visibility::Public,
            Some(FactNodeKind::Class),
        );
        assert_eq!(
            constructor_dispatch(ProgrammingLanguage::Dart, factory),
            DispatchKind::Dynamic
        );

        let class_only = target(
            FactNodeKind::Class,
            Some("class Target"),
            Visibility::Public,
            None,
        );
        assert_eq!(
            constructor_dispatch(ProgrammingLanguage::Java, class_only),
            DispatchKind::Unknown
        );
        assert_eq!(
            constructor_dispatch(ProgrammingLanguage::Dart, class_only),
            DispatchKind::Dynamic
        );
    }
}
