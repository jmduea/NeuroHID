//! # XDF 1.0 Writer
//!
//! Writes session folder (manifest, config, streams/) to a single XDF 1.0 file.
//! Implements the [XDF 1.0 spec](https://github.com/sccn/xdf/wiki/Specifications):
//! FileHeader, StreamHeader per stream, Samples and optional ClockOffset, StreamFooter.
//! All multi-byte values are little-endian.

use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

use neurohid_types::{
    error::Result,
    recording::SessionManifest,
    signal::Sample,
};

const XDF_MAGIC: &[u8; 4] = b"XDF:";
// Tag scheme used by pyxdf and xdf_rs: 1=FileHeader, 2=StreamHeader, 3=Samples, 4=ClockOffset, 5=Boundary, 6=StreamFooter.
const TAG_FILE_HEADER: u16 = 1;
const TAG_STREAM_HEADER: u16 = 2;
const TAG_SAMPLES: u16 = 3;
const TAG_STREAM_FOOTER: u16 = 6;

/// Writes a variable-length length field: 1 byte (1, 4, or 8) then length in LE.
fn write_var_len(w: &mut impl Write, len: u64) -> std::io::Result<()> {
    if len < 0x80 {
        w.write_all(&[1u8])?;
        w.write_all(&[len as u8])?;
    } else if len <= u32::MAX as u64 {
        w.write_all(&[4u8])?;
        w.write_all(&(len as u32).to_le_bytes())?;
    } else {
        w.write_all(&[8u8])?;
        w.write_all(&len.to_le_bytes())?;
    }
    Ok(())
}

/// Writes one XDF chunk: [NumLengthBytes][Length][Tag 2 bytes LE][Content].
/// Per spec, Length is the number of bytes following (Tag + Content).
fn write_chunk(w: &mut impl Write, tag: u16, content: &[u8]) -> std::io::Result<()> {
    let remainder_len = 2u64 + content.len() as u64;
    write_var_len(w, remainder_len)?;
    w.write_all(&tag.to_le_bytes())?;
    w.write_all(content)?;
    Ok(())
}

/// Build minimal FileHeader XML (version 1).
fn file_header_xml() -> String {
    r#"<?xml version="1.0"?>
<info>
    <version>1</version>
</info>"#
        .to_string()
}

/// Build StreamHeader XML for one EEG stream (channel_format float32).
fn stream_header_xml(
    _stream_id: u32,
    name: &str,
    channel_count: usize,
    nominal_srate: f64,
    source_id: Option<&str>,
    session_id: Option<&str>,
) -> String {
    let source_id = source_id.unwrap_or("neurohid");
    let session_id = session_id.unwrap_or("default");
    format!(
        r#"<?xml version="1.0"?>
<info>
    <name>{}</name>
    <type>EEG</type>
    <channel_count>{}</channel_count>
    <nominal_srate>{}</nominal_srate>
    <channel_format>float32</channel_format>
    <source_id>{}</source_id>
    <version>1</version>
    <session_id>{}</session_id>
    <desc/>
</info>"#,
        name,
        channel_count,
        nominal_srate,
        source_id,
        session_id
    )
}

/// Build StreamFooter XML (first_timestamp, last_timestamp, sample_count).
fn stream_footer_xml(
    first_timestamp_sec: f64,
    last_timestamp_sec: f64,
    sample_count: u64,
) -> String {
    format!(
        r#"<?xml version="1.0"?>
<info>
    <first_timestamp>{}</first_timestamp>
    <last_timestamp>{}</last_timestamp>
    <sample_count>{}</sample_count>
</info>"#,
        first_timestamp_sec, last_timestamp_sec, sample_count
    )
}

/// One stream's data read from a session folder (stream_*.jsonl).
struct StreamData {
    stream_id: u32,
    name: String,
    samples: Vec<Sample>,
    nominal_srate: f64,
}

fn load_stream_file(path: &Path, stream_id: u32) -> std::io::Result<StreamData> {
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("stream")
        .to_string();
    let f = File::open(path)?;
    let reader = BufReader::new(f);
    let mut samples: Vec<Sample> = Vec::new();
    for line in reader.lines() {
        let line = line?;
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Ok(s) = serde_json::from_str::<Sample>(line) {
            samples.push(s);
        }
    }
    // Nominal srate: 0 = irregular (we don't have config in this fn; caller can override).
    Ok(StreamData {
        stream_id,
        name,
        samples,
        nominal_srate: 0.0,
    })
}

/// Write Samples chunk: [StreamID u32][NumSamples var][Sample...], each Sample [0x08][f64 ts][f32...].
fn write_samples_chunk(w: &mut impl Write, stream_id: u32, samples: &[Sample]) -> std::io::Result<()> {
    if samples.is_empty() {
        return Ok(());
    }
    let _channel_count = samples[0].values.len();
    let mut buf = Vec::new();
    buf.extend_from_slice(&stream_id.to_le_bytes());
    // NumSamples as var-length: use 4 bytes for count (common case).
    let n = samples.len() as u64;
    write_var_len(&mut buf, n)?;
    for s in samples {
        // TimeStampBytes = 8, then timestamp in seconds
        buf.push(8u8);
        let ts_sec = (s.system_timestamp as f64) / 1_000_000.0;
        buf.extend_from_slice(&ts_sec.to_le_bytes());
        for &v in &s.values {
            buf.extend_from_slice(&v.to_le_bytes());
        }
    }
    write_chunk(w, TAG_SAMPLES, &buf)?;
    Ok(())
}

/// Export a session folder to a single XDF 1.0 file.
///
/// Reads `session_dir` (manifest.json, config.json, streams/*.jsonl) and writes
/// one .xdf with FileHeader, one stream per stream_*.jsonl (StreamHeader, Samples, StreamFooter).
/// Actions are not included in the XDF; they remain in the session folder as actions.jsonl.
pub fn export_session_to_xdf(session_dir: &Path, out_path: &Path) -> Result<()> {
    let manifest_path = session_dir.join("manifest.json");
    let manifest_json = std::fs::read_to_string(&manifest_path).map_err(|e| {
        neurohid_types::Error::internal(format!("read manifest: {}", e))
    })?;
    let manifest: SessionManifest = serde_json::from_str(&manifest_json).map_err(|e| {
        neurohid_types::Error::internal(format!("parse manifest: {}", e))
    })?;

    let streams_dir = session_dir.join("streams");
    if !streams_dir.is_dir() {
        return Err(neurohid_types::Error::internal("session streams/ directory missing"));
    }

    let mut stream_files: Vec<_> = std::fs::read_dir(streams_dir)
        .map_err(|e| neurohid_types::Error::internal(format!("read streams dir: {}", e)))?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path().extension().is_some_and(|ext| ext == "jsonl")
        })
        .collect();
    stream_files.sort_by_key(|e| e.path());

    let mut streams: Vec<StreamData> = Vec::new();
    for (i, entry) in stream_files.iter().enumerate() {
        let path = entry.path();
        let stream_id = (i + 1) as u32; // XDF stream IDs are typically 1-based
        let mut data = load_stream_file(&path, stream_id)
            .map_err(|e| neurohid_types::Error::internal(format!("load stream {:?}: {}", path, e)))?;
        if data.samples.is_empty() {
            continue;
        }
        data.nominal_srate = infer_nominal_srate(&data.samples);
        streams.push(data);
    }

    let mut out = File::create(out_path).map_err(|e| {
        neurohid_types::Error::internal(format!("create output file: {}", e))
    })?;

    // Magic
    out.write_all(XDF_MAGIC)?;

    // FileHeader
    let fh_xml = file_header_xml();
    write_chunk(&mut out, TAG_FILE_HEADER, fh_xml.as_bytes())?;

    let session_id = Some(manifest.session_id.as_str());

    // StreamHeaders (content = StreamID 4 bytes + XML)
    for s in &streams {
        let source_id = s.samples.first().and_then(|x| x.source_id.as_deref());
        let xml = stream_header_xml(
            s.stream_id,
            &s.name,
            s.samples.first().map_or(0, |x| x.values.len()),
            s.nominal_srate,
            source_id,
            session_id,
        );
        let mut header_content = s.stream_id.to_le_bytes().to_vec();
        header_content.extend_from_slice(xml.as_bytes());
        write_chunk(&mut out, TAG_STREAM_HEADER, &header_content)?;
    }

    // Samples (one chunk per stream)
    for s in &streams {
        write_samples_chunk(&mut out, s.stream_id, &s.samples)?;
    }

    // StreamFooters (content = StreamID 4 bytes + XML)
    for s in &streams {
        let (first_sec, last_sec) = if let (Some(a), Some(b)) = (s.samples.first(), s.samples.last()) {
            (
                (a.system_timestamp as f64) / 1_000_000.0,
                (b.system_timestamp as f64) / 1_000_000.0,
            )
        } else {
            (0.0, 0.0)
        };
        let xml = stream_footer_xml(first_sec, last_sec, s.samples.len() as u64);
        let mut footer_content = s.stream_id.to_le_bytes().to_vec();
        footer_content.extend_from_slice(xml.as_bytes());
        write_chunk(&mut out, TAG_STREAM_FOOTER, &footer_content)?;
    }

    out.sync_all().map_err(|e| {
        neurohid_types::Error::internal(format!("sync output: {}", e))
    })?;
    Ok(())
}

/// Infer nominal sampling rate from timestamps (Hz); 0 if irregular or too few samples.
fn infer_nominal_srate(samples: &[Sample]) -> f64 {
    if samples.len() < 2 {
        return 0.0;
    }
    let first = samples.first().map(|s| s.system_timestamp).unwrap_or(0);
    let last = samples.last().map(|s| s.system_timestamp).unwrap_or(0);
    let span_us = (last - first).max(1);
    let n = (samples.len() - 1) as f64;
    let interval_us = (span_us as f64) / n;
    if interval_us <= 0.0 {
        return 0.0;
    }
    1_000_000.0 / interval_us
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn file_header_xml_contains_version() {
        let xml = file_header_xml();
        assert!(xml.contains("<version>1</version>"));
    }

    #[test]
    fn stream_footer_xml_contains_sample_count() {
        let xml = stream_footer_xml(0.0, 10.0, 100);
        assert!(xml.contains("<sample_count>100</sample_count>"));
    }

    #[test]
    fn export_produces_xdf_readable_by_xdf_crate() {
        let temp = std::env::temp_dir().join("neurohid_xdf_test");
        let _ = std::fs::remove_dir_all(&temp);
        let session_dir = temp.join("session_1");
        let streams_dir = session_dir.join("streams");
        std::fs::create_dir_all(&streams_dir).unwrap();
        std::fs::write(
            session_dir.join("manifest.json"),
            r#"{"session_id":"session_1","started_at_us":1000000,"ended_at_us":2000000,"config_ref":"config.json","format_version":"1"}"#,
        )
        .unwrap();
        let sample_line = r#"{"source_id":"t","device_timestamp":null,"system_timestamp":1000000,"sequence_number":null,"values":[1.0,2.0],"quality":null}"#;
        std::fs::File::create(streams_dir.join("stream_0.jsonl"))
            .unwrap()
            .write_all(sample_line.as_bytes())
            .unwrap();
        let out = temp.join("out.xdf");
        export_session_to_xdf(&session_dir, &out).unwrap();
        let bytes = std::fs::read(&out).unwrap();
        let xdf_file = xdf::XDFFile::from_bytes(&bytes).unwrap();
        assert!(!xdf_file.streams.is_empty(), "at least one stream");
        let _ = std::fs::remove_dir_all(&temp);
    }
}
