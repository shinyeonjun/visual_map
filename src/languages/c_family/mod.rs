//! C 계열 언어 분석기.
//!
//! C와 C++가 공유하는 전처리·선언·정의·include·구조 범위 분석은 `common`에서
//! 수행하고, C와 C++의 고유 문법은 개별 모듈에서 추가한다.

pub mod c;
pub mod common;
pub mod cpp;
