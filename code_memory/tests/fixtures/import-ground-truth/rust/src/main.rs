mod local;

use crate::local::LocalType;
use std::collections::HashMap;
use missing_crate::MissingType;
// use commented_crate::Fake;

fn main() {
    let _ = LocalType;
    let _: HashMap<String, String> = HashMap::new();
    let _ = std::mem::size_of::<MissingType>();
}
