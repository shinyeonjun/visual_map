//! 여러 언어 분석기가 공유하는 선언 메타데이터 추출기.

mod declarations;
mod identifiers;

pub(crate) use declarations::{
    declaration_body, declaration_header, extract_modifiers, extract_parameters,
    extract_return_type, extract_visibility, is_exported,
};
pub(crate) use identifiers::{
    file_module_name, kind_key, node_name, node_span, node_text, qualified_name, stable_id,
};
