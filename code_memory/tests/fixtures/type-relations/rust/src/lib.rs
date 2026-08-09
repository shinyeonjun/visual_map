pub struct Payload;

pub struct ResultValue;

pub trait Contract {
    fn execute(&self, input: Payload) -> ResultValue;
}

pub struct Service {
    pub current: Payload,
}

impl Contract for Service {
    fn execute(&self, input: Payload) -> ResultValue {
        let transient: Payload = input;
        let _ = &self.current;
        let _ = transient;
        ResultValue
    }
}

pub struct Store<T: Contract> {
    pub value: T,
}

impl<T: Contract> Store<T> {
    pub fn value(&self) -> &T {
        &self.value
    }
}
