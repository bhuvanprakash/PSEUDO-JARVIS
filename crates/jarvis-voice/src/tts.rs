use tokio::process::Command;

/// Speak text using local TTS (`espeak`, `spd-say`, or `say` on macOS).
pub async fn speak_local(text: &str) -> jarvis_core::Result<()> {
    for (bin, args) in [
        ("spd-say", vec![text]),
        ("espeak", vec![text]),
        ("say", vec![text]),
    ] {
        let mut cmd = Command::new(bin);
        for arg in args {
            cmd.arg(arg);
        }
        if let Ok(status) = cmd.status().await {
            if status.success() {
                return Ok(());
            }
        }
    }
    Err(jarvis_core::error::JarvisError::Other(
        "no local TTS found (install espeak, spd-say, or use macOS say)".into(),
    ))
}
