use std::collections::HashSet;

use serde::{Deserialize, Serialize};

/// An enum representing the type of a message type argument.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ArgumentType {
    Text,
    Number,
    Enum(Vec<(String, String)>),
}

/// A single argument for a command.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Argument {
    pub name: String,
    pub description: String,
    pub argument_type: ArgumentType,
}

/// The rules for a message type argument, which defines what values are valid for the argument and how it should be displayed in the client.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum CommandExecution {
    /// The macro is inserted directly. This is done directly.
    Macro {
        replacement: String,
        /// The position of the caret after insertion, if specified. If not specified, the caret will be placed at the end of the inserted text.
        caret_position: Option<u32>,
    },
    /// The arguments are substituted into the template string, which is then inserted into the message. The template can contain placeholders in the format {argument_name}, which will be replaced with the corresponding argument value when the command is executed.
    /// This is done when the message is actually sent.
    Format(String),
}

/// Defines how the output of a command should be treated. This is used by the client to figure out what to do with the result of a command.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub enum CommandOutput {
    /// The output should be treated as plain text and inserted into the message.
    Text,
    /// The output should be treated as a message type, which can trigger special effects or behaviors in the client.
    MessageType,
    /// The output should not be displayed to the user. This can be used for commands that perform actions without producing visible output, e.g., a command that saves data to a database or triggers an API call without returning anything to the user.
    None,
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
    pub output: CommandOutput,
}

impl Command {
    pub fn new_macro(
        name: impl ToString,
        replacement: impl ToString,
        caret_position: Option<u32>,
    ) -> Self {
        let replacement = replacement.to_string();

        Command {
            name: name.to_string(),
            source: "Built-in".to_string(),
            description: format!("Inserts '{}'", replacement),
            usage: format!("/{}", name.to_string()),
            arguments: vec![],
            execution: CommandExecution::Macro {
                replacement,
                caret_position,
            },
            output: CommandOutput::Text,
        }
    }

    /// Checks if the command is a macro and returns the replacement string if it is.
    pub fn is_macro(&self) -> Option<(String, Option<u32>)> {
        if let CommandExecution::Macro {
            replacement,
            caret_position,
        } = &self.execution
        {
            Some((replacement.clone(), *caret_position))
        } else {
            None
        }
    }

    pub fn generate_usage(&mut self) {
        if let CommandExecution::Macro { replacement, .. } = &self.execution {
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

    pub fn validate_template_structure(&self) -> Result<(), String> {
        let CommandExecution::Format(template) = &self.execution else {
            return Ok(());
        };

        let mut placeholders = HashSet::new();
        let mut inside_bracket = false;
        let mut current_placeholder = String::new();

        for c in template.chars() {
            match c {
                '{' => {
                    if inside_bracket {
                        return Err(
                            "Malformed template: Nested opening brackets '{' discovered."
                                .to_string(),
                        );
                    }
                    inside_bracket = true;
                }
                '}' => {
                    if !inside_bracket {
                        return Err(
                            "Malformed template: Unmatched closing bracket '}' discovered."
                                .to_string(),
                        );
                    }
                    if current_placeholder.is_empty() {
                        return Err(
                            "Malformed template: Found an empty placeholder '{}'.".to_string()
                        );
                    }
                    placeholders.insert(current_placeholder.clone());
                    current_placeholder.clear();
                    inside_bracket = false;
                }
                _ if inside_bracket => {
                    current_placeholder.push(c);
                }
                _ => {}
            }
        }

        if inside_bracket {
            return Err("Malformed template: Trailing unclosed opening bracket '{'.".to_string());
        }

        let defined_args: HashSet<String> =
            self.arguments.iter().map(|arg| arg.name.clone()).collect();

        let extra_in_template: Vec<&String> = placeholders.difference(&defined_args).collect();
        if !extra_in_template.is_empty() {
            let names: Vec<String> = extra_in_template
                .iter()
                .map(|s| format!("'{}'", s))
                .collect();
            return Err(format!(
                "Template references variables not declared in arguments: {}",
                names.join(", ")
            ));
        }

        let missing_from_template: Vec<&String> = defined_args.difference(&placeholders).collect();
        if !missing_from_template.is_empty() {
            let names: Vec<String> = missing_from_template
                .iter()
                .map(|s| format!("'{}'", s))
                .collect();
            return Err(format!(
                "Declared arguments are missing from the template layout: {}",
                names.join(", ")
            ));
        }

        Ok(())
    }
}

pub fn default_commands() -> Vec<Command> {
    let mut cmds: Vec<Command> = vec![
        // Command {
        //     name: "link".to_string(),
        //     source: "Built-in".to_string(),
        //     description: "Formats a link.".to_string(),
        //     usage: String::new(),
        //     arguments: vec![
        //         Argument {
        //             name: "text".to_string(),
        //             description: "The text to display for the link".to_string(),
        //             argument_type: ArgumentType::Text,
        //         },
        //         Argument {
        //             name: "url".to_string(),
        //             description: "The URL the link should point to".to_string(),
        //             argument_type: ArgumentType::Text,
        //         },
        //     ],
        //     execution: CommandExecution::Format("[{text}]({url})".to_string()),
        //     output: CommandOutput::Text,
        // },
        // Command {
        //     name: "effect".to_string(),
        //     source: "Built-in".to_string(),
        //     description: "Adds the selected effect to the message".to_string(),
        //     usage: "".to_string(),
        //     arguments: vec![Argument {
        //         name: "effect".to_string(),
        //         description: "The effect to apply to the message".to_string(),
        //         argument_type: ArgumentType::Enum(vec![
        //             ("confetti".to_string(), "nic.custom.confetti".to_string()),
        //             ("fireworks".to_string(), "nic.custom.fireworks".to_string()),
        //             (
        //                 "rainfall".to_string(),
        //                 "io.element.effect.rainfall".to_string(),
        //             ),
        //             (
        //                 "snowfall".to_string(),
        //                 "io.element.effect.snowfall".to_string(),
        //             ),
        //             (
        //                 "spaceinvaders".to_string(),
        //                 "io.element.effects.space_invaders".to_string(),
        //             ),
        //         ]),
        //     }],
        //     execution: CommandExecution::Format("{effect}".to_string()),
        //     output: CommandOutput::MessageType,
        // },
        // Command {
        //     name: "spoiler".to_string(),
        //     source: "Built-in".to_string(),
        //     description: "Hides text behind a spoiler.".to_string(),
        //     usage: "".to_string(),
        //     arguments: vec![Argument {
        //         name: "text".to_string(),
        //         description: "The secret text to hide".to_string(),
        //         argument_type: ArgumentType::Text,
        //     }],
        //     execution: CommandExecution::Format("||{text}||".to_string()),
        //     output: CommandOutput::Text,
        // },
        // Command {
        //     name: "strikethrough".to_string(),
        //     source: "Built-in".to_string(),
        //     description: "Strikes through the text.".to_string(),
        //     usage: "".to_string(),
        //     arguments: vec![Argument {
        //         name: "text".to_string(),
        //         description: "The text to strike through".to_string(),
        //         argument_type: ArgumentType::Text,
        //     }],
        //     execution: CommandExecution::Format("~~{text}~~".to_string()),
        //     output: CommandOutput::Text,
        // },
        // Command {
        //     name: "italic".to_string(),
        //     source: "Built-in".to_string(),
        //     description: "Italics the text.".to_string(),
        //     usage: "".to_string(),
        //     arguments: vec![Argument {
        //         name: "text".to_string(),
        //         description: "The text to italicize".to_string(),
        //         argument_type: ArgumentType::Text,
        //     }],
        //     execution: CommandExecution::Format("*{text}*".to_string()),
        //     output: CommandOutput::Text,
        // },
        // Command {
        //     name: "bold".to_string(),
        //     source: "Built-in".to_string(),
        //     description: "Bolds the text.".to_string(),
        //     usage: "".to_string(),
        //     arguments: vec![Argument {
        //         name: "text".to_string(),
        //         description: "The text to bold".to_string(),
        //         argument_type: ArgumentType::Text,
        //     }],
        //     execution: CommandExecution::Format("**{text}**".to_string()),
        //     output: CommandOutput::Text,
        // },
    ];

    for cmd in cmds.iter_mut() {
        cmd.generate_usage();
    }

    cmds
}

pub fn default_macros() -> Vec<Command> {
    vec![
        Command::new_macro("shrug", "¯\\_(ツ)_/¯", None),
        Command::new_macro("tableflip", "(╯°□°）╯︵ ┻━┻", None),
        Command::new_macro("unflip", "┬─┬ ノ(^_^ノ)", None),
        Command::new_macro("lenny", "( ͡° ͜ʖ ͡°)", None),
        Command::new_macro("disapprove", "ಠ_ಠ", None),
        Command::new_macro("link", "[]()", Some(1)),
        Command::new_macro("spoiler", "||||", Some(2)),
        Command::new_macro("strikethrough", "~~~~", Some(2)),
        Command::new_macro("italic", "**", Some(2)),
        Command::new_macro("bold", "****", Some(2)),
    ]
}
