use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

pub(super) fn validate_import_source_path(source_path: &str) -> Result<PathBuf, String> {
    let trimmed = source_path.trim();
    if trimmed.is_empty() {
        return Err("WAV source path is required.".to_string());
    }
    let path = PathBuf::from(trimmed);
    if !path.exists() {
        return Err("WAV source file does not exist.".to_string());
    }
    let metadata = path
        .metadata()
        .map_err(|error| format!("WAV source file is not readable: {error}"))?;
    if !metadata.is_file() {
        return Err("WAV source path must be a file.".to_string());
    }
    if path
        .extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| !extension.eq_ignore_ascii_case("wav"))
        .unwrap_or(true)
    {
        return Err("WAV source file must have a .wav extension.".to_string());
    }
    Ok(path)
}

pub(super) fn validate_wav_header(path: &Path) -> Result<u32, String> {
    let mut file = std::fs::File::open(path)
        .map_err(|error| format!("WAV source file is not readable: {error}"))?;
    let file_len = file
        .metadata()
        .map_err(|error| format!("WAV source file is not readable: {error}"))?
        .len();
    let mut riff_header = [0_u8; 12];
    file.read_exact(&mut riff_header)
        .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
    if &riff_header[0..4] != b"RIFF" || &riff_header[8..12] != b"WAVE" {
        return Err("WAV source file has an unsupported WAV header.".to_string());
    }

    let mut sample_rate_hz = None;
    let mut has_data_chunk = false;
    loop {
        let mut chunk_header = [0_u8; 8];
        match file.read_exact(&mut chunk_header) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => return Err("WAV source file has an unsupported WAV header.".to_string()),
        }
        let chunk_size = u32::from_le_bytes([
            chunk_header[4],
            chunk_header[5],
            chunk_header[6],
            chunk_header[7],
        ]) as u64;
        ensure_wav_chunk_payload_available(&mut file, chunk_size, file_len)?;
        match &chunk_header[0..4] {
            b"fmt " => {
                if chunk_size < 16 {
                    return Err("WAV source file has an unsupported WAV header.".to_string());
                }
                let mut fmt = [0_u8; 16];
                file.read_exact(&mut fmt)
                    .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
                let audio_format = u16::from_le_bytes([fmt[0], fmt[1]]);
                let sample_rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
                let bits_per_sample = u16::from_le_bytes([fmt[14], fmt[15]]);
                if !matches!(audio_format, 1 | 3) || sample_rate == 0 || bits_per_sample == 0 {
                    return Err("WAV source file has an unsupported WAV header.".to_string());
                }
                sample_rate_hz = Some(sample_rate);
                seek_wav_chunk_remainder(&mut file, chunk_size - 16)?;
            }
            b"data" => {
                if chunk_size == 0 {
                    return Err("WAV source file has an unsupported WAV header.".to_string());
                }
                has_data_chunk = true;
                seek_wav_chunk_remainder(&mut file, chunk_size)?;
            }
            _ => seek_wav_chunk_remainder(&mut file, chunk_size)?,
        }
        if chunk_size % 2 == 1 {
            file.seek(SeekFrom::Current(1))
                .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
        }
    }

    match (sample_rate_hz, has_data_chunk) {
        (Some(sample_rate), true) => Ok(sample_rate),
        _ => Err("WAV source file has an unsupported WAV header.".to_string()),
    }
}

fn ensure_wav_chunk_payload_available(
    file: &mut std::fs::File,
    chunk_size: u64,
    file_len: u64,
) -> Result<(), String> {
    let payload_start = file
        .stream_position()
        .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
    let padded_size = chunk_size
        .checked_add(chunk_size % 2)
        .ok_or_else(|| "WAV source file has an unsupported WAV header.".to_string())?;
    let payload_end = payload_start
        .checked_add(padded_size)
        .ok_or_else(|| "WAV source file has an unsupported WAV header.".to_string())?;
    if payload_end > file_len {
        return Err("WAV source file has an unsupported WAV header.".to_string());
    }
    Ok(())
}

fn seek_wav_chunk_remainder(file: &mut std::fs::File, bytes: u64) -> Result<(), String> {
    let offset = i64::try_from(bytes)
        .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
    file.seek(SeekFrom::Current(offset))
        .map_err(|_| "WAV source file has an unsupported WAV header.".to_string())?;
    Ok(())
}
