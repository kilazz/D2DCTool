use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::fs;
use std::io::{Cursor, Read};
use std::path::Path;
use walkdir::WalkDir;

const DDS_MAGIC: u32 = 0x20534444; // b"DDS "
const DDSD_MIPMAPCOUNT: u32 = 0x00020000;
const DDSCAPS_MIPMAP: u32 = 0x00400000;
const DDSCAPS_COMPLEX: u32 = 0x00000008;

pub fn repair_single_dds(file_path: &Path) -> std::io::Result<bool> {
    let mut data = fs::read(file_path)?;
    let total_len = data.len();
    if total_len < 128 {
        return Ok(false);
    }

    let mut cur = Cursor::new(&mut data[..]);
    if cur.read_u32::<LittleEndian>()? != DDS_MAGIC {
        return Ok(false);
    }

    cur.set_position(8);
    let dw_flags = cur.read_u32::<LittleEndian>()?;
    let height = cur.read_u32::<LittleEndian>()?;
    let width = cur.read_u32::<LittleEndian>()?;

    cur.set_position(28);
    let current_mips = cur.read_u32::<LittleEndian>()?;

    cur.set_position(84);
    let mut four_cc = [0u8; 4];
    cur.read_exact(&mut four_cc)?;

    let block_size = match &four_cc {
        b"DXT1" => 8,
        b"DXT3" | b"DXT5" => 16,
        _ => return Ok(false),
    };

    let mut available_payload = total_len.saturating_sub(128);
    let mut actual_mips = 0u32;
    let mut w = width;
    let mut h = height;

    while available_payload > 0 {
        let bw = w.div_ceil(4);
        let bh = h.div_ceil(4);
        let level_size = (bw as usize) * (bh as usize) * block_size;

        if available_payload >= level_size && level_size > 0 {
            available_payload -= level_size;
            actual_mips += 1;
            if w == 1 && h == 1 {
                break;
            }
            w = (w / 2).max(1);
            h = (h / 2).max(1);
        } else {
            break;
        }
    }

    if actual_mips == 0 {
        return Ok(false);
    }

    let mut modified = false;

    if current_mips != actual_mips {
        cur.set_position(28);
        cur.write_u32::<LittleEndian>(actual_mips)?;
        modified = true;
    }

    let should_have_mip = actual_mips > 1;
    let has_mip = (dw_flags & DDSD_MIPMAPCOUNT) != 0;
    if should_have_mip != has_mip {
        let new_flags = if should_have_mip {
            dw_flags | DDSD_MIPMAPCOUNT
        } else {
            dw_flags & !DDSD_MIPMAPCOUNT
        };
        cur.set_position(8);
        cur.write_u32::<LittleEndian>(new_flags)?;
        modified = true;
    }

    cur.set_position(108);
    let caps = cur.read_u32::<LittleEndian>()?;
    let has_caps_mip = (caps & DDSCAPS_MIPMAP) != 0;
    if should_have_mip != has_caps_mip {
        let new_caps = if should_have_mip {
            caps | DDSCAPS_MIPMAP | DDSCAPS_COMPLEX
        } else {
            caps & !DDSCAPS_MIPMAP
        };
        cur.set_position(108);
        cur.write_u32::<LittleEndian>(new_caps)?;
        modified = true;
    }

    if modified {
        crate::tools::atomic_write(file_path, cur.get_ref())?;
    }
    Ok(modified)
}

pub fn batch_repair_dds<F: Fn(&str)>(dir: &Path, on_log: F) -> std::io::Result<usize> {
    let mut count = 0;
    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.path().is_file()
            && entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("dds"))
                .unwrap_or(false)
            && let Ok(true) = repair_single_dds(entry.path())
        {
            count += 1;
            on_log(&format!(
                "Repaired DDS: {:?}",
                entry.path().file_name().unwrap_or_default()
            ));
        }
    }
    Ok(count)
}
