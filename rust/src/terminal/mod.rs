//! Terminal layer: output formatting and interactive prompts.

pub mod output;
pub mod prompts;
pub mod spinner;

pub use output::{
    clear_screen, command_list, dim, failure, fields, heading, info, json_output, show_banner,
    success, table, usage, warn,
};
pub use prompts::{confirm, is_interactive, prompt};
