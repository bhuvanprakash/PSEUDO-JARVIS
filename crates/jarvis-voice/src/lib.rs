pub mod stt;
pub mod tts;

pub use stt::{record_microphone, transcribe_file, transcribe_with_whisper_cli};
pub use tts::speak_local;
