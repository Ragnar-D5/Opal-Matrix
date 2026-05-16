use shared::commands::{default_commands, default_macros, Command};
use tauri::command;

#[command]
pub fn get_commands() -> Vec<Command> {
    let mut default_commands = default_commands();
    default_commands.extend(default_macros());

    default_commands
}
