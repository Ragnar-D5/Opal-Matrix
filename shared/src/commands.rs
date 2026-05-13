use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum ArgumentType {
    Text,
    Number,
    Enum(Vec<String>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Argument {
    name: String,
    description: String,
    argument_type: ArgumentType,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Command {
    name: String,
    description: String,
    arguments: Vec<Argument>,
}

pub fn default_commands() -> Vec<Command> {
    vec![
        Command {
            name: "link".to_string(),
            description: "Formats a link. Usage: link [text] [url]".to_string(),
            arguments: vec![
                Argument {
                    name: "text".to_string(),
                    description: "The text to display for the link".to_string(),
                    argument_type: ArgumentType::Text,
                },
                Argument {
                    name: "url".to_string(),
                    description: "The URL the link should point to".to_string(),
                    argument_type: ArgumentType::Text,
                },
            ],
        },
        Command {
            name: "effect".to_string(),
            description: "Adds the selected effect to the message".to_string(),
            arguments: vec![Argument {
                name: "effect".to_string(),
                description: "The effect to apply to the message".to_string(),
                argument_type: ArgumentType::Enum(vec![
                    "confetti".to_string(),
                    "fireworks".to_string(),
                    "rainfall".to_string(),
                    "snowfall".to_string(),
                    "spaceinvaders".to_string(),
                ]),
            }],
        },
        Command {
            name: "spoiler".to_string(),
            description: "Hides text behind a spoiler. Usage: spoiler [text] [warning]".to_string(),
            arguments: vec![
                Argument {
                    name: "text".to_string(),
                    description: "The secret text to hide".to_string(),
                    argument_type: ArgumentType::Text,
                },
                Argument {
                    name: "warning".to_string(),
                    description: "The visible warning label (e.g., 'Endgame spoilers')".to_string(),
                    argument_type: ArgumentType::Text,
                },
            ],
        },
    ]
}
