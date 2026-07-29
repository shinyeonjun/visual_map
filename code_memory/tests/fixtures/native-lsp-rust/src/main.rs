mod types;

use types::{Entity, EntityBox, User};

fn add(left: i32, right: i32) -> i32 {
    left + right
}

fn main() {
    let box_value = EntityBox {
        value: User {
            id_value: "user-1".to_string(),
        },
    };
    let _ = add(1, box_value.get().id().len() as i32);
}
