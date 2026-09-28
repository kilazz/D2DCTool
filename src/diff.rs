use crate::logger::UiLogger;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Serialize, Deserialize)]
pub struct D2dcModPatch {
    pub patch_name: String,
    pub created_at: u64,
    pub text_diffs: BTreeMap<String, String>,
    pub binary_diffs: BTreeMap<String, String>,
}

pub fn create_diff(
    base_dir: &Path,
    mod_dir: &Path,
    patch_out: &Path,
    logger: &UiLogger,
) -> io::Result<()> {
    logger.log(&format!(
        "[*] Generating Mod Diff Patch: {:?} vs {:?}",
        base_dir, mod_dir
    ));

    let mut text_diffs = BTreeMap::new();
    let mut binary_diffs = BTreeMap::new();

    for entry in WalkDir::new(mod_dir).into_iter().filter_map(|e| e.ok()) {
        if !entry.path().is_file() {
            continue;
        }

        let rel_path = entry
            .path()
            .strip_prefix(mod_dir)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?
            .to_string_lossy()
            .replace('\\', "/");

        let base_file = base_dir.join(&rel_path);
        let mod_bytes = fs::read(entry.path())?;

        let is_modified = if base_file.exists() {
            let base_bytes = fs::read(&base_file)?;
            base_bytes != mod_bytes
        } else {
            true
        };

        if is_modified {
            if (rel_path.ends_with(".xml") || rel_path.ends_with(".txt"))
                && let Ok(text_content) = String::from_utf8(mod_bytes.clone())
            {
                text_diffs.insert(rel_path.clone(), text_content);
                logger.log(&format!("[+] Tracked modified text file: {}", rel_path));
                continue;
            }
            let b64 = simple_base64_encode(&mod_bytes);
            binary_diffs.insert(rel_path.clone(), b64);
            logger.log(&format!(
                "[+] Tracked modified binary resource: {}",
                rel_path
            ));
        }
    }

    let patch = D2dcModPatch {
        patch_name: patch_out
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string(),
        created_at: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0),
        text_diffs,
        binary_diffs,
    };

    let f = File::create(patch_out)?;
    serde_json::to_writer_pretty(f, &patch)?;

    logger.log(&format!(
        "[+] Diff patch successfully created! (Text: {}, Binary: {})",
        patch.text_diffs.len(),
        patch.binary_diffs.len()
    ));
    Ok(())
}

pub fn apply_patch(target_dir: &Path, patch_path: &Path, logger: &UiLogger) -> io::Result<()> {
    logger.log(&format!(
        "[*] Applying Diff Patch: {:?} -> {:?}",
        patch_path, target_dir
    ));

    let patch_str = fs::read_to_string(patch_path)?;
    let patch: D2dcModPatch = serde_json::from_str(&patch_str)
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

    let mut applied_count = 0;

    for (rel, content) in patch.text_diffs {
        let dest = target_dir.join(&rel);
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p)?;
        }
        crate::tools::atomic_write(&dest, content.as_bytes())?;
        applied_count += 1;
    }

    for (rel, b64) in patch.binary_diffs {
        let dest = target_dir.join(&rel);
        if let Some(p) = dest.parent() {
            fs::create_dir_all(p)?;
        }
        if let Some(bytes) = simple_base64_decode(&b64) {
            crate::tools::atomic_write(&dest, &bytes)?;
            applied_count += 1;
        }
    }

    logger.log(&format!(
        "[+] Mod patch applied successfully! {} files written/updated.",
        applied_count
    ));
    Ok(())
}

fn simple_base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);

        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(((b1 & 0x0F) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[(b2 & 0x3F) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn simple_base64_decode(s: &str) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = 0u32;
    let mut bits = 0;

    for &b in s.as_bytes() {
        let val = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' | b'\r' | b'\n' | b' ' => continue,
            _ => return None,
        };
        buf = (buf << 6) | (val as u32);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((buf >> bits) as u8);
        }
    }
    Some(out)
}
