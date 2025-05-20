pub enum Value<T> {
    Here(T),
    Moved,
}

impl<T> Value<T> {
    /// extracts value
    /// can be performed only once if Value::Here, otherwise it will panic
    pub fn extract(&mut self) -> T {
        let mut moved = Value::Moved;
        std::mem::swap(&mut moved, self);

        match moved {
            Value::Here(val) => return val,
            Value::Moved => panic!("was already moved"),
        }
    }

    pub fn contains(&self) -> bool {
        match self {
            Value::Here(_) => true,
            Value::Moved => false,
        }
    }
}
