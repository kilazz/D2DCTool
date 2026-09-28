use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use zopfli::{Format, Options};

const BOUNDARY_SIZE: u64 = 32768; // 32 KB alignment boundary

pub struct Dv2Entry {
    pub name: String,
    pub start_offset: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
}

/// Reads the directory index and filename table from a DV2 archive without loading payloads into RAM.
pub fn read_entries(dv2_path: &Path) -> Result<Vec<Dv2Entry>, String> {
    let f = File::open(dv2_path).map_err(|e| format!("Failed to open DV2 archive: {}", e))?;
    let mut reader = BufReader::new(f);

    let version = reader
        .read_u32::<LittleEndian>()
        .map_err(|e| format!("Failed to read DV2 version: {}", e))?;
    if version != 4 && version != 5 {
        return Err(format!("Unsupported DV2 archive version: {}", version));
    }

    if version == 5 {
        reader
            .seek(SeekFrom::Current(8))
            .map_err(|e| format!("Failed to seek past V5 header padding: {}", e))?;
    }

    reader
        .seek(SeekFrom::Current(2))
        .map_err(|e| format!("Failed to seek past alignment flags: {}", e))?;
    let _data_start_offset = reader
        .read_u32::<LittleEndian>()
        .map_err(|e| format!("Failed to read data start offset: {}", e))?;
    let filenames_size = reader
        .read_u32::<LittleEndian>()
        .map_err(|e| format!("Failed to read filename table length: {}", e))?
        as usize;

    let mut name_bytes = vec![0u8; filenames_size];
    reader
        .read_exact(&mut name_bytes)
        .map_err(|e| format!("Failed to read filename table: {}", e))?;

    let mut names = Vec::new();
    let mut current_start = 0;
    for i in 0..name_bytes.len() {
        if name_bytes[i] == 0 {
            if i > current_start {
                names.push(String::from_utf8_lossy(&name_bytes[current_start..i]).into_owned());
            }
            current_start = i + 1;
        }
    }

    let file_count = reader
        .read_u32::<LittleEndian>()
        .map_err(|e| format!("Failed to read archive file count: {}", e))?
        as usize;
    let mut entries = Vec::with_capacity(file_count);

    for i in 0..file_count {
        let start_offset = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| format!("Failed to read entry start offset: {}", e))?;
        let compressed_size = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| format!("Failed to read entry compressed size: {}", e))?;
        let uncompressed_size = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| format!("Failed to read entry uncompressed size: {}", e))?;

        if i < names.len() {
            entries.push(Dv2Entry {
                name: names[i].clone(),
                start_offset,
                compressed_size,
                uncompressed_size,
            });
        }
    }

    Ok(entries)
}

/// Streams and unpacks all entries from a DV2 archive into the specified output directory.
pub fn unpack_dv2<F: Fn(&str)>(
    dv2_path: &Path,
    out_dir: &Path,
    on_progress: F,
) -> Result<(), String> {
    let f = File::open(dv2_path).map_err(|e| format!("Failed to open DV2 archive: {}", e))?;
    let mut reader = BufReader::new(f);

    let version = reader
        .read_u32::<LittleEndian>()
        .map_err(|e| format!("Failed to read DV2 version: {}", e))?;
    if version != 4 && version != 5 {
        return Err(format!("Unsupported DV2 archive version: {}", version));
    }

    if version == 5 {
        reader
            .seek(SeekFrom::Current(8))
            .map_err(|e| format!("Failed to seek past V5 header padding: {}", e))?;
    }

    reader
        .seek(SeekFrom::Current(2))
        .map_err(|e| format!("Failed to seek past alignment flags: {}", e))?;
    let data_start_offset = reader
        .read_u32::<LittleEndian>()
        .map_err(|e| format!("Failed to read data start offset: {}", e))?
        as u64;
    let filenames_size = reader
        .read_u32::<LittleEndian>()
        .map_err(|e| format!("Failed to read filename table length: {}", e))?
        as usize;

    let mut name_bytes = vec![0u8; filenames_size];
    reader
        .read_exact(&mut name_bytes)
        .map_err(|e| format!("Failed to read filename table: {}", e))?;

    let mut names = Vec::new();
    let mut current_start = 0;
    for i in 0..name_bytes.len() {
        if name_bytes[i] == 0 {
            if i > current_start {
                names.push(String::from_utf8_lossy(&name_bytes[current_start..i]).into_owned());
            }
            current_start = i + 1;
        }
    }

    let file_count = reader
        .read_u32::<LittleEndian>()
        .map_err(|e| format!("Failed to read archive file count: {}", e))?
        as usize;
    let mut entries = Vec::with_capacity(file_count);

    for i in 0..file_count {
        let start_offset = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| format!("Failed to read entry start offset: {}", e))?;
        let compressed_size = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| format!("Failed to read entry compressed size: {}", e))?;
        let uncompressed_size = reader
            .read_u32::<LittleEndian>()
            .map_err(|e| format!("Failed to read entry uncompressed size: {}", e))?;

        if i < names.len() {
            entries.push(Dv2Entry {
                name: names[i].clone(),
                start_offset,
                compressed_size,
                uncompressed_size,
            });
        }
    }

    fs::create_dir_all(out_dir).map_err(|e| format!("Failed to create output directory: {}", e))?;

    for (i, entry) in entries.iter().enumerate() {
        on_progress(&format!(
            "Extracting ({}/{}): {}",
            i + 1,
            entries.len(),
            entry.name
        ));

        let rel_path = entry.name.replace('\\', "/");
        let out_path = out_dir.join(&rel_path);

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                format!(
                    "Failed to create directory structure for {}: {}",
                    rel_path, e
                )
            })?;
        }

        reader
            .seek(SeekFrom::Start(
                data_start_offset + entry.start_offset as u64,
            ))
            .map_err(|e| format!("Failed to seek to entry offset for {}: {}", entry.name, e))?;

        let out_file = File::create(&out_path).map_err(|e| {
            format!(
                "Failed to create destination file {}: {}",
                out_path.display(),
                e
            )
        })?;
        let mut out_writer = BufWriter::new(out_file);

        let take_reader = (&mut reader).take(entry.compressed_size as u64);

        if entry.uncompressed_size > 0 {
            let mut decoder = ZlibDecoder::new(take_reader);
            std::io::copy(&mut decoder, &mut out_writer)
                .map_err(|e| format!("Decompression error on file {}: {}", entry.name, e))?;
        } else {
            let mut limited = take_reader;
            std::io::copy(&mut limited, &mut out_writer)
                .map_err(|e| format!("Error copying raw file {}: {}", entry.name, e))?;
        }
        out_writer
            .flush()
            .map_err(|e| format!("Failed to flush extracted file {}: {}", entry.name, e))?;
    }

    on_progress("Unpack completed successfully!");
    Ok(())
}

/// Recursively searches for and unpacks all .dv2 archives in a directory tree.
pub fn batch_unpack_dv2<F: Fn(&str)>(root_dir: &Path, on_progress: F) -> Result<usize, String> {
    on_progress(&format!("[*] Scanning for DV2 archives in: {:?}", root_dir));
    let mut found = 0;

    for entry in walkdir::WalkDir::new(root_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.path().is_file()
            && entry
                .path()
                .extension()
                .and_then(|s| s.to_str())
                .map(|ext| ext.eq_ignore_ascii_case("dv2"))
                .unwrap_or(false)
        {
            let p = entry.path();
            let file_stem = p.file_stem().unwrap_or_default().to_string_lossy();
            let parent = p.parent().unwrap_or(root_dir);
            let out_dir = parent.join(format!("{}_extracted", file_stem));

            on_progress(&format!(
                "Extracting: {:?}",
                p.file_name().unwrap_or_default()
            ));
            if let Err(e) = unpack_dv2(p, &out_dir, |_| {}) {
                on_progress(&format!(
                    "[!] Error unpacking {:?}: {}",
                    p.file_name().unwrap_or_default(),
                    e
                ));
            } else {
                found += 1;
            }
        }
    }

    on_progress(&format!(
        "[+] Batch unpack completed: successfully processed {} archives.",
        found
    ));
    Ok(found)
}

/// Packs a single directory into a DV2 archive supporting deduplication, Zlib and Zopfli.
pub fn pack_dv2<F: Fn(&str)>(
    source_dir: &Path,
    out_dv2_path: &Path,
    compress: bool,
    algo: &str,
    comp_level: u32,
    on_progress: F,
) -> Result<(), String> {
    let mut files = Vec::new();
    for entry in walkdir::WalkDir::new(source_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.path().is_file() {
            files.push(entry.path().to_path_buf());
        }
    }

    if files.is_empty() {
        return Err("Source directory is empty or contains no files to pack.".into());
    }

    let mut names_ms = Vec::new();
    let mut entries = Vec::new();

    for file in &files {
        let relative_path = file
            .strip_prefix(source_dir)
            .map_err(|e| format!("Failed to resolve relative path for {:?}: {}", file, e))?
            .to_string_lossy()
            .replace('/', "\\");

        names_ms.extend_from_slice(relative_path.as_bytes());
        names_ms.push(0);
        entries.push(Dv2Entry {
            name: relative_path,
            start_offset: 0,
            compressed_size: 0,
            uncompressed_size: 0,
        });
    }

    let mut out_file = File::create(out_dv2_path)
        .map_err(|e| format!("Failed to create destination DV2 archive: {}", e))?;

    let header_size = 22u32;
    let file_count = entries.len() as u32;
    let dir_block_size = file_count * 12;

    let total_header_area = header_size + names_ms.len() as u32 + 4 + dir_block_size;
    let data_start_offset = get_next_boundary(total_header_area as u64) as u32;

    out_file
        .write_u32::<LittleEndian>(5)
        .map_err(|e| e.to_string())?;
    out_file
        .write_u32::<LittleEndian>(1)
        .map_err(|e| e.to_string())?;
    out_file
        .write_u32::<LittleEndian>(4)
        .map_err(|e| e.to_string())?;
    out_file.write_u8(0).map_err(|e| e.to_string())?;
    out_file.write_u8(1).map_err(|e| e.to_string())?;
    out_file
        .write_u32::<LittleEndian>(data_start_offset)
        .map_err(|e| e.to_string())?;
    out_file
        .write_u32::<LittleEndian>(names_ms.len() as u32)
        .map_err(|e| e.to_string())?;

    out_file.write_all(&names_ms).map_err(|e| e.to_string())?;
    out_file
        .write_u32::<LittleEndian>(file_count)
        .map_err(|e| e.to_string())?;

    let dir_position = out_file.stream_position().map_err(|e| e.to_string())?;
    let placeholder = vec![0u8; dir_block_size as usize];
    out_file
        .write_all(&placeholder)
        .map_err(|e| e.to_string())?;

    pad_to(&mut out_file, data_start_offset as u64)?;

    let mut current_offset = 0u32;
    let mut seen_payloads: HashMap<(u32, [u8; 32]), (u32, u32)> = HashMap::new();
    let mut dedup_count = 0usize;
    let mut saved_bytes = 0u64;

    let use_zopfli = compress && algo.eq_ignore_ascii_case("zopfli");

    for i in 0..files.len() {
        on_progress(&format!(
            "Packing ({}/{}): {}{}",
            i + 1,
            files.len(),
            entries[i].name,
            if use_zopfli { " [Zopfli]" } else { "" }
        ));

        let file_bytes = fs::read(&files[i])
            .map_err(|e| format!("Failed to read file {:?}: {}", files[i], e))?;
        let file_length = file_bytes.len() as u32;

        let mut hasher = Sha256::new();
        hasher.update(&file_bytes);
        let hash: [u8; 32] = hasher.finalize().into();

        if let Some(&(existing_off, comp_sz)) = seen_payloads.get(&(file_length, hash)) {
            dedup_count += 1;
            saved_bytes += file_length as u64;
            entries[i].start_offset = existing_off;
            entries[i].compressed_size = comp_sz;
            entries[i].uncompressed_size = if compress { file_length } else { 0 };
            continue;
        }

        entries[i].start_offset = current_offset;

        if compress {
            entries[i].uncompressed_size = file_length;

            if use_zopfli {
                let iters = match comp_level {
                    0..=2 => 2,
                    3..=5 => 5,
                    6..=8 => 15,
                    _ => 40,
                };

                let options = Options {
                    iteration_count: std::num::NonZeroU64::new(iters as u64).unwrap(),
                    ..Default::default()
                };

                let mut comp_data = Vec::new();
                zopfli::compress(options, Format::Zlib, &file_bytes[..], &mut comp_data).map_err(
                    |e| format!("Zopfli compression error on {}: {}", entries[i].name, e),
                )?;

                out_file.write_all(&comp_data).map_err(|e| e.to_string())?;
                entries[i].compressed_size = comp_data.len() as u32;
            } else {
                let start_pos = out_file.stream_position().map_err(|e| e.to_string())?;
                {
                    let mut encoder = ZlibEncoder::new(&mut out_file, Compression::new(comp_level));
                    encoder
                        .write_all(&file_bytes)
                        .map_err(|e| format!("Zlib write error: {}", e))?;
                    encoder
                        .finish()
                        .map_err(|e| format!("Zlib finalization error: {}", e))?;
                }
                let end_pos = out_file.stream_position().map_err(|e| e.to_string())?;
                entries[i].compressed_size = (end_pos - start_pos) as u32;
            }
        } else {
            entries[i].uncompressed_size = 0;
            entries[i].compressed_size = file_length;
            out_file.write_all(&file_bytes).map_err(|e| e.to_string())?;
        }

        seen_payloads.insert(
            (file_length, hash),
            (entries[i].start_offset, entries[i].compressed_size),
        );
        current_offset += entries[i].compressed_size;

        if !compress {
            let cur_pos = out_file.stream_position().map_err(|e| e.to_string())?;
            let padded_pos = get_next_boundary(cur_pos);
            current_offset += (padded_pos - cur_pos) as u32;
            pad_to(&mut out_file, padded_pos)?;
        }
    }

    out_file
        .seek(SeekFrom::Start(dir_position))
        .map_err(|e| format!("Failed to seek back to directory table: {}", e))?;

    for entry in &entries {
        out_file
            .write_u32::<LittleEndian>(entry.start_offset)
            .map_err(|e| e.to_string())?;
        out_file
            .write_u32::<LittleEndian>(entry.compressed_size)
            .map_err(|e| e.to_string())?;
        out_file
            .write_u32::<LittleEndian>(entry.uncompressed_size)
            .map_err(|e| e.to_string())?;
    }

    if dedup_count > 0 {
        on_progress(&format!(
            "Deduplication saved: {} redundant files merged ({:.2} MB saved).",
            dedup_count,
            saved_bytes as f64 / 1_048_576.0
        ));
    }

    on_progress("DV2 packaging completed successfully!");
    Ok(())
}

/// Scans for all directories ending with `_extracted` in root_dir and packs each into a `.dv2` archive.
pub fn batch_pack_folders<F: Fn(&str)>(
    root_dir: &Path,
    compress: bool,
    algo: &str,
    comp_level: u32,
    on_progress: F,
) -> Result<usize, String> {
    on_progress(&format!(
        "[*] Scanning for extracted folders in: {:?}",
        root_dir
    ));
    let mut compiled = 0;

    let entries = fs::read_dir(root_dir).map_err(|e| e.to_string())?;
    for entry in entries.filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_dir() {
            let folder_name = path
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();

            if !folder_name.to_lowercase().ends_with("_extracted") {
                continue;
            }

            let base_name = &folder_name[..folder_name.len().saturating_sub(10)];
            let out_dv2 = root_dir.join(format!("{}.dv2", base_name));

            on_progress(&format!("Compiling folder: {:?}", folder_name));
            if let Err(e) = pack_dv2(&path, &out_dv2, compress, algo, comp_level, |_| {}) {
                on_progress(&format!("[!] Error packing {:?}: {}", folder_name, e));
            } else {
                compiled += 1;
            }
        }
    }

    on_progress(&format!(
        "[+] Batch pack completed: successfully compiled {} archives.",
        compiled
    ));
    Ok(compiled)
}

fn get_next_boundary(pos: u64) -> u64 {
    pos.div_ceil(BOUNDARY_SIZE) * BOUNDARY_SIZE
}

fn pad_to(stream: &mut File, target_position: u64) -> Result<(), String> {
    let current = stream.stream_position().map_err(|e| e.to_string())?;
    if target_position > current {
        let diff = target_position - current;
        let padding = vec![0u8; diff as usize];
        stream.write_all(&padding).map_err(|e| e.to_string())?;
    }
    Ok(())
}
