pub mod tts;
pub mod weather;
pub mod clipboard;
pub mod personality;
pub mod identity;
pub mod obsidian;
#[path = "link-download.rs"]
pub mod link_download;
pub mod convert;
pub mod dates;
pub mod embeddings;
pub mod retrieval;
pub mod fuzzy;

pub use tts::TTSService;
pub use fuzzy::fuzzy_score;
