use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use xxhash_rust::xxh3::xxh3_128;
use zopfli::{Format, Options};

const BOUNDARY_SIZE: u64 = 32768; // 32 KB alignment boundary
const READ_BUF_CAPACITY: usize = 128 * 1024; // 128 KB read buffer
const WRITE_BUF_CAPACITY: usize = 256 * 1024; // 256 KB write buffer

pub struct Dv2Entry {
    pub name: String,
    pub start_offset: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
}

/// Reads the directory index and filename table from a DV2 archive without loading payloads into RAM.
pub fn read_entries(dv2_path: &Path) -> Result<Vec<Dv2Entry>, String> {
    let f = File::open(dv2_path).map_err(|e| format!("Failed to open DV2 archive: {}", e))?;
    let mut reader = BufReader::with_capacity(READ_BUF_CAPACITY, f);

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

/// Streams and unpacks all entries from a single DV2 archive into the specified destination folder.
pub fn unpack_dv2<F: Fn(&str)>(
    dv2_path: &Path,
    out_dir: &Path,
    on_progress: F,
) -> Result<(), String> {
    let f = File::open(dv2_path).map_err(|e| format!("Failed to open DV2 archive: {}", e))?;
    let mut reader = BufReader::with_capacity(READ_BUF_CAPACITY, f);

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
        let mut out_writer = BufWriter::with_capacity(WRITE_BUF_CAPACITY, out_file);

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

/// Fully parallelized batch unpacker across all CPU cores preserving directory hierarchy.
pub fn batch_unpack_dv2<F: Fn(&str) + Sync + Send>(
    source_root: &Path,
    dest_root: &Path,
    on_progress: F,
) -> Result<usize, String> {
    on_progress(&format!(
        "[*] Scanning for DV2 archives in: {:?}",
        source_root
    ));

    let mut dv2_files = Vec::new();
    for entry in walkdir::WalkDir::new(source_root)
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
            dv2_files.push(entry.path().to_path_buf());
        }
    }

    let total = dv2_files.len();
    on_progress(&format!(
        "[*] Found {} archives. Extracting in parallel on all CPU cores...",
        total
    ));

    let success_count = AtomicUsize::new(0);
    let processed_count = AtomicUsize::new(0);

    dv2_files.par_iter().for_each(|dv2_path| {
        let rel_path = match dv2_path.strip_prefix(source_root) {
            Ok(p) => p,
            Err(_) => return,
        };

        let parent_rel = rel_path.parent().unwrap_or_else(|| Path::new(""));
        let file_stem = dv2_path.file_stem().unwrap_or_default().to_string_lossy();
        let folder_name = format!("{}_extracted", file_stem);
        let target_out_dir = dest_root.join(parent_rel).join(folder_name);

        let is_ok = unpack_dv2(dv2_path, &target_out_dir, |_| {}).is_ok();
        let done = processed_count.fetch_add(1, Ordering::Relaxed) + 1;

        if is_ok {
            success_count.fetch_add(1, Ordering::Relaxed);
        }

        if done.is_multiple_of(5) || done == total {
            on_progress(&format!(
                "Progress: {}/{} archives processed...",
                done, total
            ));
        }
    });

    let final_success = success_count.load(Ordering::Relaxed);
    on_progress(&format!(
        "[+] Batch unpack completed: {}/{} archives successfully extracted.",
        final_success, total
    ));
    Ok(final_success)
}

struct PreparedChunkItem {
    index: usize,
    uncompressed_size: u32,
    compressed_bytes: Vec<u8>,
    hash: u128,
}

/// Packs a directory into a DV2 archive using parallel chunked compression and instant XXH3 deduplication.
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

    let file_out = File::create(out_dv2_path)
        .map_err(|e| format!("Failed to create destination DV2 archive: {}", e))?;
    let mut out_file = BufWriter::with_capacity(WRITE_BUF_CAPACITY, file_out);

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

    pad_to_writer(&mut out_file, data_start_offset as u64)?;

    let mut current_offset = 0u32;
    let mut seen_payloads: HashMap<(u32, u128), (u32, u32)> = HashMap::new();
    let mut dedup_count = 0usize;
    let mut saved_bytes = 0u64;

    let use_zopfli = compress && algo.eq_ignore_ascii_case("zopfli");

    on_progress(&format!(
        "Packing {} files (Algorithm: {}, Level: {}, Dedup: XXH3)...",
        files.len(),
        if use_zopfli { "Zopfli" } else { "Zlib" },
        comp_level
    ));

    // Process files in parallel batches of 64 to keep memory strictly bounded while utilizing all cores
    const BATCH_SIZE: usize = 64;
    for (chunk_idx, file_chunk) in files.chunks(BATCH_SIZE).enumerate() {
        let base_idx = chunk_idx * BATCH_SIZE;

        // Step 1: Read, compute XXH3-128 and compress files in parallel across all CPU cores
        let processed: Vec<PreparedChunkItem> = file_chunk
            .par_iter()
            .enumerate()
            .filter_map(|(sub_idx, path)| {
                let file_bytes = fs::read(path).ok()?;
                let uncompressed_size = file_bytes.len() as u32;
                let hash = xxh3_128(&file_bytes);

                let compressed_bytes = if compress {
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
                        let mut out = Vec::new();
                        zopfli::compress(options, Format::Zlib, &file_bytes[..], &mut out).ok()?;
                        out
                    } else {
                        let mut encoder =
                            ZlibEncoder::new(Vec::new(), Compression::new(comp_level));
                        encoder.write_all(&file_bytes).ok()?;
                        encoder.finish().ok()?
                    }
                } else {
                    file_bytes
                };

                Some(PreparedChunkItem {
                    index: base_idx + sub_idx,
                    uncompressed_size,
                    compressed_bytes,
                    hash,
                })
            })
            .collect();

        // Step 2: Sequentially write compressed blocks and apply deduplication
        for item in processed {
            let i = item.index;
            let file_length = item.uncompressed_size;

            if let Some(&(existing_off, comp_sz)) = seen_payloads.get(&(file_length, item.hash)) {
                dedup_count += 1;
                saved_bytes += file_length as u64;
                entries[i].start_offset = existing_off;
                entries[i].compressed_size = comp_sz;
                entries[i].uncompressed_size = if compress { file_length } else { 0 };
                continue;
            }

            entries[i].start_offset = current_offset;
            entries[i].uncompressed_size = if compress { file_length } else { 0 };
            entries[i].compressed_size = item.compressed_bytes.len() as u32;

            out_file
                .write_all(&item.compressed_bytes)
                .map_err(|e| e.to_string())?;

            seen_payloads.insert(
                (file_length, item.hash),
                (entries[i].start_offset, entries[i].compressed_size),
            );
            current_offset += entries[i].compressed_size;

            if !compress {
                let cur_pos = out_file.stream_position().map_err(|e| e.to_string())?;
                let padded_pos = get_next_boundary(cur_pos);
                current_offset += (padded_pos - cur_pos) as u32;
                pad_to_writer(&mut out_file, padded_pos)?;
            }
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

    out_file
        .flush()
        .map_err(|e| format!("Failed to flush archive: {}", e))?;

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

/// Parallelized batch packing: compiles extracted directories in parallel.
pub fn batch_pack_folders<F: Fn(&str) + Sync + Send>(
    source_extracted_root: &Path,
    dest_packed_root: &Path,
    compress: bool,
    algo: &str,
    comp_level: u32,
    on_progress: F,
) -> Result<usize, String> {
    on_progress(&format!(
        "[*] Scanning for extracted folders at all depths in: {:?}",
        source_extracted_root
    ));

    let mut extracted_dirs = Vec::new();
    let mut it = walkdir::WalkDir::new(source_extracted_root).into_iter();

    loop {
        let entry = match it.next() {
            None => break,
            Some(Err(_)) => continue,
            Some(Ok(entry)) => entry,
        };

        let path = entry.path();
        if path.is_dir() && path != source_extracted_root {
            let dir_name = path.file_name().unwrap_or_default().to_string_lossy();
            if dir_name.to_lowercase().ends_with("_extracted") {
                it.skip_current_dir();
                extracted_dirs.push(path.to_path_buf());
            }
        }
    }

    let total = extracted_dirs.len();
    on_progress(&format!(
        "[*] Found {} folders. Compiling archives in parallel across all CPU cores...",
        total
    ));

    let compiled_count = AtomicUsize::new(0);
    let processed_count = AtomicUsize::new(0);

    extracted_dirs.par_iter().for_each(|dir_path| {
        let dir_name = dir_path.file_name().unwrap_or_default().to_string_lossy();
        let parent_dir = dir_path.parent().unwrap_or(source_extracted_root);
        let parent_rel = match parent_dir.strip_prefix(source_extracted_root) {
            Ok(p) => p,
            Err(_) => return,
        };

        let clean_stem = &dir_name[..dir_name.len().saturating_sub(10)];
        let target_dv2_path = dest_packed_root
            .join(parent_rel)
            .join(format!("{}.dv2", clean_stem));

        if let Some(p) = target_dv2_path.parent() {
            let _ = fs::create_dir_all(p);
        }

        let is_ok = pack_dv2(
            dir_path,
            &target_dv2_path,
            compress,
            algo,
            comp_level,
            |_| {},
        )
        .is_ok();
        let done = processed_count.fetch_add(1, Ordering::Relaxed) + 1;

        if is_ok {
            compiled_count.fetch_add(1, Ordering::Relaxed);
        }

        if done.is_multiple_of(5) || done == total {
            on_progress(&format!(
                "Progress: {}/{} archives compiled...",
                done, total
            ));
        }
    });

    let final_compiled = compiled_count.load(Ordering::Relaxed);
    on_progress(&format!(
        "[+] Batch pack completed: {}/{} archives successfully compiled.",
        final_compiled, total
    ));
    Ok(final_compiled)
}

fn get_next_boundary(pos: u64) -> u64 {
    pos.div_ceil(BOUNDARY_SIZE) * BOUNDARY_SIZE
}

fn pad_to_writer<W: Write + Seek>(stream: &mut W, target_position: u64) -> Result<(), String> {
    let current = stream.stream_position().map_err(|e| e.to_string())?;
    if target_position > current {
        let diff = target_position - current;
        let padding = vec![0u8; diff as usize];
        stream.write_all(&padding).map_err(|e| e.to_string())?;
    }
    Ok(())
}
