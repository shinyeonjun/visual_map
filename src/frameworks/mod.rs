//! 프레임워크 감지와 프레임워크 문법 보강 영역.
//!
//! 언어 분석기가 추출한 일반 구문을 프레임워크의 의미 있는 사실로 보강한다.
//! 예를 들어 route 등록, controller annotation, ORM repository를 공통 사실로
//! 변환하지만, 도메인 그룹화 자체는 담당하지 않는다.

pub mod adapters;
pub mod common;
pub mod registry;
