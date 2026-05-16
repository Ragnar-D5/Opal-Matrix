use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArgumentType {
    Text,
    Number,
    Enum(Vec<String>),
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Argument {
    pub name: String,
    pub description: String,
    pub argument_type: ArgumentType,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum CommandExecution {
    Macro(String),
    Regex(String),
}

/// Represents a custom command that users can invoke in their messages, e.g., /link or /effect.
/// Can also be a macro, which is a simpler command that just replaces the command text with a predefined string.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Command {
    pub name: String,
    pub source: String,
    pub usage: String,
    pub description: String,
    pub arguments: Vec<Argument>,
    pub execution: CommandExecution,
}

impl Command {
    pub fn new_macro(name: impl ToString, replacement: impl ToString) -> Self {
        let replacement = replacement.to_string();

        Command {
            name: name.to_string(),
            source: "Built-in".to_string(),
            description: format!("Inserts '{}'", &replacement),
            usage: format!("/{}", name.to_string()),
            arguments: vec![],
            execution: CommandExecution::Macro(replacement),
        }
    }

    pub fn generate_usage(&mut self) {
        if let CommandExecution::Macro(replacement) = &self.execution {
            self.usage = replacement.clone();
            return;
        }

        let args_usage = self
            .arguments
            .iter()
            .map(|arg| format!("[{}]", arg.name))
            .collect::<Vec<String>>()
            .join(" ");

        self.usage = format!("/{} {}", self.name, args_usage).trim().to_string();
    }
}

pub fn default_commands() -> Vec<Command> {
    let mut cmds = vec![
        Command {
            name: "link".to_string(),
            source: "Built-in".to_string(),
            description: "Formats a link.".to_string(),
            usage: String::new(),
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
            execution: CommandExecution::Regex(r"\[([^\]]+)\]\(([^)]+)\)".to_string()),
        },
        Command {
            name: "effect".to_string(),
            source: "Built-in".to_string(),
            description: "Adds the selected effect to the message".to_string(),
            usage: "".to_string(),
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
            execution: CommandExecution::Regex(r"\[effect=(\w+)\]".to_string()),
        },
        Command {
            name: "spoiler".to_string(),
            source: "Built-in".to_string(),
            description: "Hides text behind a spoiler.".to_string(),
            usage: "".to_string(),
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
            execution: CommandExecution::Regex(r"\|\|([^|]+)\|\|".to_string()),
        },
    ];

    for cmd in cmds.iter_mut() {
        cmd.generate_usage();
    }

    cmds
}

pub fn default_macros() -> Vec<Command> {
    vec![
        Command::new_macro("shrug", "¯\\_(ツ)_/¯"),
        Command::new_macro("tableflip", "(╯°□°）╯︵ ┻━┻"),
        Command::new_macro("unflip", "┬─┬ ノ(^_^ノ)"),
        Command::new_macro("lenny", "( ͡° ͜ʖ ͡°)"),
        Command::new_macro("disapprove", "ಠ_ಠ"),
    ]
}
