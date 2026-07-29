pub trait Entity {
    fn id(&self) -> &str;
}

pub struct User {
    pub id_value: String,
}

impl Entity for User {
    fn id(&self) -> &str {
        &self.id_value
    }
}

pub struct EntityBox<T: Entity> {
    pub value: T,
}

impl<T: Entity> EntityBox<T> {
    pub fn get(&self) -> &T {
        &self.value
    }
}
