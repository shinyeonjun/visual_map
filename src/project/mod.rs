//! 프로젝트 경로 검증, 파일 인벤토리, 스냅샷 식별을 담당한다.

mod fingerprint;
mod model;
mod scanner;

pub use model::ScanOutput;
pub use scanner::ProjectScanner;
