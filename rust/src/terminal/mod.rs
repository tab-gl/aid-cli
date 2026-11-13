//! Terminal layer: output formatting and interactive prompts.

pub mod output;
pub mod prompts;
pub mod spinner;

pub use output::{
    clear_screen, command_list, dim, failure, faint, fields, heading, info, json_output,
    show_banner, success, table, text, usage, warn,
};
pub use prompts::{confirm, is_interactive, prompt};
