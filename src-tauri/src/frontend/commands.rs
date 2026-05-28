use log::warn;
use shared::commands::{Command, default_commands, default_macros};
use tauri::command;

#[command]
pub fn get_commands() -> Vec<Command> {
    let mut default_commands = default_commands();
    default_commands.extend(default_macros());

    default_commands
        .into_iter()
        .filter(|cmd| {
            cmd.validate_template_structure()
                .map_err(|e| warn!("Invalid command: {e}"))
                .is_ok()
        })
        .collect()
}
