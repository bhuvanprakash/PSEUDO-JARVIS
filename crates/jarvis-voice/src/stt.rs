use std::path::Path;
use std::process::Stdio;

use tokio::process::Command;

/// Transcribe audio using the `whisper` CLI if installed.
pub async fn transcribe_with_whisper_cli(audio_path: &Path) -> jarvis_core::Result<String> {
    let output = Command::new("whisper")
        .arg(audio_path)
        .args(["--model", "base", "--output_format", "txt"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await;

    if let Ok(out) = output {
        if out.status.success() {
            let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if !text.is_empty() {
                return Ok(text);
            }
        }
    }

    Err(jarvis_core::error::JarvisError::Other(
        "whisper CLI not found; install openai-whisper or use `jarvis voice \"phrase\"`".into(),
    ))
}

pub async fn transcribe_file(audio_path: &Path) -> jarvis_core::Result<String> {
    if !audio_path.exists() {
        return Err(jarvis_core::error::JarvisError::Other(format!(
            "audio file not found: {}",
            audio_path.display()
        )));
    }
    transcribe_with_whisper_cli(audio_path).await
}

/// Record audio from microphone using `arecord` (Linux) or `ffmpeg`.
pub async fn record_microphone(output: &Path, seconds: u32) -> jarvis_core::Result<()> {
    if let Ok(status) = Command::new("arecord")
        .args([
            "-q",
            "-d",
            &seconds.to_string(),
            "-f",
            "cd",
            "-t",
            "wav",
            &output.to_string_lossy(),
        ])
        .status()
        .await
    {
        if status.success() {
            return Ok(());
        }
    }
    if let Ok(status) = Command::new("ffmpeg")
        .args([
            "-y",
            "-f",
            "alsa",
            "-i",
            "default",
            "-t",
            &seconds.to_string(),
            &output.to_string_lossy(),
        ])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .await
    {
        if status.success() {
            return Ok(());
        }
    }
    Err(jarvis_core::error::JarvisError::Other(
        "could not record audio; install arecord or ffmpeg".into(),
    ))
}
