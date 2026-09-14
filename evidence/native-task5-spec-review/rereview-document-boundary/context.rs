pub mod config { pub use llmgw::config::*; }
pub mod lifecycle { pub mod platform {pub fn current_user_identity()->std::io::Result<String>{panic!("review native identity adapter must never execute")}} }
#[path="autostart_source_copy.rs"]mod autostart;
