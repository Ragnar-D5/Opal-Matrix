use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub description: String,
    pub replacement: String,
}

impl Macro {
    pub fn new(name: impl Into<String>, replacement: impl Into<String>) -> Self {
        let replacement: String = replacement.into();

        Macro {
            name: name.into(),
            description: format!("Inserts '{}'", replacement.clone()),
            replacement: replacement,
        }
    }
}

pub fn default_macros() -> Vec<Macro> {
    vec![
        Macro::new("shrug", "¯\\_(ツ)_/¯"),
        Macro::new("tableflip", "(╯°□°）╯︵ ┻━┻"),
        Macro::new("unflip", "┬─┬ ノ(^_^ノ)"),
        Macro::new("lenny", "( ͡° ͜ʖ ͡°)"),
        Macro::new("disapprove", "ಠ_ಠ"),
    ]
}
