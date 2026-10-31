#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum BorderStyle {
    #[default]
    Single,
    Double,
    Heavy,
}

impl BorderStyle {
    /// Box-drawing line weight: 1 light, 2 heavy, 3 double.
    pub fn weight(&self) -> u8 {
        match self {
            BorderStyle::Single => 1,
            BorderStyle::Heavy  => 2,
            BorderStyle::Double => 3,
        }
    }
}
