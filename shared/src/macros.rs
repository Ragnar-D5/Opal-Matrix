use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Macro {
    pub name: String,
    pub description: String,
    pub replacement: String,
}

impl Macro {
    pub fn new(name: String, replacment: String) -> Self {
        Macro {
            name,
            description: format!("Inserts '{}'", replacment),
            replacement: replacment,
        }
    }
}

pub fn default_macros() -> Vec<Macro> {
    vec![
        Macro::new("shrug".to_string(), "¯\\_(ツ)_/¯".to_string()),
        Macro::new("tableflip".to_string(), "(╯°□°）╯︵ ┻━┻".to_string()),
        Macro::new("unflip".to_string(), "┬─┬ ノ(^_^ノ)".to_string()),
        Macro::new("lenny".to_string(), "( ͡° ͜ʖ ͡°)".to_string()),
        Macro::new("disapprove".to_string(), "ಠ_ಠ".to_string()),
    ]
}
