pub mod config { pub use llmgw::config::*; }
pub mod lifecycle { pub mod platform {pub fn current_user_identity()->std::io::Result<String>{panic!("native identity adapter must never be called in review")}} }
#[path="autostart_source_copy.rs"]mod autostart;
