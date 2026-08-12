//! ECMAScript 계열 언어 분석기.
//!
//! JavaScript와 TypeScript가 공유하는 문법·의미 추출은 `common`에서 수행하고,
//! 각 언어의 고유한 사실은 개별 모듈에서 추가한다.

pub mod common;
pub mod javascript;
pub mod typescript;
