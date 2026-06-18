use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use std::fs::{self, File};
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;

const BOUNDARY_SIZE: u64 = 32768; // 32 KB boundary

pub struct Dv2Entry {
    pub name: String,
    pub start_offset: u32,
    pub compressed_size: u32,
    pub uncompressed_size: u32,
}

pub fn read_entries(dv2_path: &Path) -> Result<Vec<Dv2Entry>, String> {
    let mut f = File::open(dv2_path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    f.read_to_end(&mut data).map_err(|e| e.to_string())?;

    let mut cursor = Cursor::new(&data);

    let version = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())?;
    if version != 4 && version != 5 {
        return Err(format!("Unsupported DV2 version: {}", version));
    }

    if version == 5 {
        cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?; // unknown1
        cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?; // unknown2
    }

    let _is_aligned = cursor.read_u8().map_err(|e| e.to_string())?;
    let _is_packed = cursor.read_u8().map_err(|e| e.to_string())?;
    let _data_start_offset = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())?;
    let filenames_size = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())? as usize;

    let mut name_bytes = vec![0u8; filenames_size];
    cursor
        .read_exact(&mut name_bytes)
        .map_err(|e| e.to_string())?;

    let mut names = Vec::new();
    let mut current_start = 0;
    for i in 0..name_bytes.len() {
        if name_bytes[i] == 0 {
            if i > current_start {
                let s = String::from_utf8_lossy(&name_bytes[current_start..i]).into_owned();
                names.push(s);
            }
            current_start = i + 1;
        }
    }

    let file_count = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())? as usize;
    let mut entries = Vec::with_capacity(file_count);

    for i in 0..file_count {
        let start_offset = cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?;
        let compressed_size = cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?;
        let uncompressed_size = cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?;

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

pub fn unpack_dv2<F: Fn(&str)>(
    dv2_path: &Path,
    out_dir: &Path,
    on_progress: F,
) -> Result<(), String> {
    let mut f = File::open(dv2_path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    f.read_to_end(&mut data).map_err(|e| e.to_string())?;

    let mut cursor = Cursor::new(&data);

    let version = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())?;
    if version != 4 && version != 5 {
        return Err(format!("Unsupported DV2 version: {}", version));
    }

    if version == 5 {
        cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?; // unknown1
        cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?; // unknown2
    }

    let _is_aligned = cursor.read_u8().map_err(|e| e.to_string())?;
    let _is_packed = cursor.read_u8().map_err(|e| e.to_string())?;
    let data_start_offset = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())? as u64;
    let filenames_size = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())? as usize;

    let mut name_bytes = vec![0u8; filenames_size];
    cursor
        .read_exact(&mut name_bytes)
        .map_err(|e| e.to_string())?;

    let mut names = Vec::new();
    let mut current_start = 0;
    for i in 0..name_bytes.len() {
        if name_bytes[i] == 0 {
            if i > current_start {
                let s = String::from_utf8_lossy(&name_bytes[current_start..i]).into_owned();
                names.push(s);
            }
            current_start = i + 1;
        }
    }

    let file_count = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())? as usize;
    let mut entries = Vec::with_capacity(file_count);

    for i in 0..file_count {
        let start_offset = cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?;
        let compressed_size = cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?;
        let uncompressed_size = cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?;

        if i < names.len() {
            entries.push(Dv2Entry {
                name: names[i].clone(),
                start_offset,
                compressed_size,
                uncompressed_size,
            });
        }
    }

    fs::create_dir_all(out_dir).map_err(|e| e.to_string())?;

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
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }

        cursor.set_position(data_start_offset + entry.start_offset as u64);
        let mut out_file = File::create(&out_path).map_err(|e| e.to_string())?;

        if entry.uncompressed_size > 0 {
            let comp_data = &data[cursor.position() as usize
                ..(cursor.position() + entry.compressed_size as u64) as usize];
            let mut decoder = ZlibDecoder::new(comp_data);
            let mut decompressed = Vec::with_capacity(entry.uncompressed_size as usize);
            decoder
                .read_to_end(&mut decompressed)
                .map_err(|e| e.to_string())?;
            out_file
                .write_all(&decompressed)
                .map_err(|e| e.to_string())?;
        } else {
            let raw_data = &data[cursor.position() as usize
                ..(cursor.position() + entry.compressed_size as u64) as usize];
            out_file.write_all(raw_data).map_err(|e| e.to_string())?;
        }
    }

    on_progress("Unpack complete!");
    Ok(())
}

pub fn pack_dv2<F: Fn(&str)>(
    source_dir: &Path,
    out_dv2_path: &Path,
    compress: bool,
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

    let mut names_ms = Vec::new();
    let mut entries = Vec::new();

    for file in &files {
        let relative_path = file
            .strip_prefix(source_dir)
            .unwrap()
            .to_string_lossy()
            .replace("/", "\\");
        names_ms.extend_from_slice(relative_path.as_bytes());
        names_ms.push(0); // Null terminator
        entries.push(Dv2Entry {
            name: relative_path,
            start_offset: 0,
            compressed_size: 0,
            uncompressed_size: 0,
        });
    }

    let mut out_file = File::create(out_dv2_path).map_err(|e| e.to_string())?;

    let header_size = 22u32; // V5 header size
    let file_count = entries.len() as u32;
    let dir_block_size = file_count * 12;

    let total_header_area = header_size + names_ms.len() as u32 + 4 + dir_block_size;
    let data_start_offset = get_next_boundary(total_header_area as u64) as u32;

    // Write V5 Header
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
    out_file
        .write_all(&vec![0u8; dir_block_size as usize])
        .map_err(|e| e.to_string())?;

    pad_to(&mut out_file, data_start_offset as u64)?;

    let mut current_offset = 0u32;

    for i in 0..files.len() {
        on_progress(&format!(
            "Packing ({}/{}): {}",
            i + 1,
            files.len(),
            entries[i].name
        ));
        entries[i].start_offset = current_offset;

        let mut in_file = File::open(&files[i]).map_err(|e| e.to_string())?;
        let file_length = in_file.metadata().map_err(|e| e.to_string())?.len();

        if compress {
            entries[i].uncompressed_size = file_length as u32;
            let start_pos = out_file.stream_position().map_err(|e| e.to_string())?;

            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::new(comp_level));
            let mut file_buf = Vec::new();
            in_file
                .read_to_end(&mut file_buf)
                .map_err(|e| e.to_string())?;
            encoder.write_all(&file_buf).map_err(|e| e.to_string())?;
            let comp_data = encoder.finish().map_err(|e| e.to_string())?;
            out_file.write_all(&comp_data).map_err(|e| e.to_string())?;

            entries[i].compressed_size =
                (out_file.stream_position().map_err(|e| e.to_string())? - start_pos) as u32;
        } else {
            entries[i].uncompressed_size = 0;
            entries[i].compressed_size = file_length as u32;
            std::io::copy(&mut in_file, &mut out_file).map_err(|e| e.to_string())?;
        }

        current_offset += entries[i].compressed_size;

        if !compress {
            let padded_pos =
                get_next_boundary(out_file.stream_position().map_err(|e| e.to_string())?);
            current_offset +=
                (padded_pos - out_file.stream_position().map_err(|e| e.to_string())?) as u32;
            pad_to(&mut out_file, padded_pos)?;
        }
    }

    out_file
        .seek(SeekFrom::Start(dir_position))
        .map_err(|e| e.to_string())?;
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

    on_progress("Pack complete!");
    Ok(())
}

fn get_next_boundary(pos: u64) -> u64 {
    pos.div_ceil(BOUNDARY_SIZE) * BOUNDARY_SIZE
}

fn pad_to(stream: &mut File, target_position: u64) -> Result<(), String> {
    let current = stream.stream_position().map_err(|e| e.to_string())?;
    if target_position > current {
        let diff = target_position - current;
        stream
            .write_all(&vec![0u8; diff as usize])
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}
