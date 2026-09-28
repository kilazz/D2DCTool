use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub fn convert_dds_to_nif<F: Fn(&str)>(
    dds_path: &Path,
    nif_path: &Path,
    on_progress: F,
) -> Result<(), String> {
    let mut fs = File::open(dds_path).map_err(|e| e.to_string())?;
    let metadata = fs.metadata().map_err(|e| e.to_string())?;
    if metadata.len() < 128 {
        return Err("File is too small to be a valid DDS.".into());
    }

    let mut magic = [0u8; 4];
    fs.read_exact(&mut magic).map_err(|e| e.to_string())?;
    if &magic != b"DDS " {
        return Err("Invalid DDS magic signature.".into());
    }

    fs.seek(SeekFrom::Start(12)).map_err(|e| e.to_string())?;
    let height = fs.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;
    let width = fs.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;

    fs.seek(SeekFrom::Start(28)).map_err(|e| e.to_string())?;
    let mut mip_map_count = fs.read_u32::<LittleEndian>().map_err(|e| e.to_string())?;

    fs.seek(SeekFrom::Start(84)).map_err(|e| e.to_string())?;
    let mut four_cc = [0u8; 4];
    fs.read_exact(&mut four_cc).map_err(|e| e.to_string())?;

    if &four_cc == b"DX10" {
        return Err(
            "DX10 extended DDS format is not supported by Gamebryo (expected DXT1, DXT3, or DXT5)."
                .into(),
        );
    }

    let block_size: u32;
    let pixel_format: u32;

    if &four_cc == b"DXT1" {
        block_size = 8;
        pixel_format = 4;
    } else if &four_cc == b"DXT3" {
        block_size = 16;
        pixel_format = 5;
    } else if &four_cc == b"DXT5" {
        block_size = 16;
        pixel_format = 6;
    } else {
        return Err(format!(
            "Unsupported DDS format: {:?}. Only DXT1, DXT3, and DXT5 are supported.",
            String::from_utf8_lossy(&four_cc)
        ));
    }

    if mip_map_count == 0 {
        let max_dim = std::cmp::max(width, height);
        mip_map_count = (max_dim as f64).log2() as u32 + 1;
    }

    on_progress(&format!(
        "DDS Info: {}x{}, FourCC: {}, MipMaps: {}",
        width,
        height,
        String::from_utf8_lossy(&four_cc),
        mip_map_count
    ));

    let mut mipmaps = Vec::new();
    let mut current_offset = 0u32;
    let mut w = width;
    let mut h = height;

    for _ in 0..mip_map_count {
        let block_width = std::cmp::max(1, w.div_ceil(4));
        let block_height = std::cmp::max(1, h.div_ceil(4));
        let size = block_width * block_height * block_size;

        mipmaps.push((w, h, current_offset));
        current_offset += size;

        w = std::cmp::max(1, w / 2);
        h = std::cmp::max(1, h / 2);
    }

    let pixel_data_size = current_offset;

    let out_fs = File::create(nif_path).map_err(|e| e.to_string())?;
    let mut bw = BufWriter::new(out_fs);

    let header_string = b"Gamebryo File Format, Version 20.3.0.9\n";
    bw.write_all(header_string).map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(0x14030009)
        .map_err(|e| e.to_string())?;
    bw.write_u8(1).map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(0x20000)
        .map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(1).map_err(|e| e.to_string())?;
    bw.write_u16::<LittleEndian>(1).map_err(|e| e.to_string())?;

    let block_type = "NiPersistentSrcTextureRendererData";
    bw.write_u32::<LittleEndian>(block_type.len() as u32)
        .map_err(|e| e.to_string())?;
    bw.write_all(block_type.as_bytes())
        .map_err(|e| e.to_string())?;
    bw.write_u16::<LittleEndian>(0).map_err(|e| e.to_string())?;

    let block_size_nif = 87 + (mip_map_count * 12) + pixel_data_size;
    bw.write_u32::<LittleEndian>(block_size_nif)
        .map_err(|e| e.to_string())?;

    bw.write_u32::<LittleEndian>(0).map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(0).map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(0).map_err(|e| e.to_string())?;

    bw.write_u32::<LittleEndian>(pixel_format)
        .map_err(|e| e.to_string())?;
    bw.write_u8(0).map_err(|e| e.to_string())?;
    bw.write_i32::<LittleEndian>(-1)
        .map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(0).map_err(|e| e.to_string())?;
    bw.write_u8(1).map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(0).map_err(|e| e.to_string())?;
    bw.write_u8(0).map_err(|e| e.to_string())?;

    // Channel 1
    bw.write_u32::<LittleEndian>(4).map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(4).map_err(|e| e.to_string())?;
    bw.write_u8(0).map_err(|e| e.to_string())?;
    bw.write_u8(0).map_err(|e| e.to_string())?;

    // Channels 2-4
    for _ in 0..3 {
        bw.write_u32::<LittleEndian>(19)
            .map_err(|e| e.to_string())?;
        bw.write_u32::<LittleEndian>(5).map_err(|e| e.to_string())?;
        bw.write_u8(0).map_err(|e| e.to_string())?;
        bw.write_u8(0).map_err(|e| e.to_string())?;
    }

    bw.write_i32::<LittleEndian>(-1)
        .map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(mip_map_count)
        .map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(0).map_err(|e| e.to_string())?;

    for mip in mipmaps {
        bw.write_u32::<LittleEndian>(mip.0)
            .map_err(|e| e.to_string())?;
        bw.write_u32::<LittleEndian>(mip.1)
            .map_err(|e| e.to_string())?;
        bw.write_u32::<LittleEndian>(mip.2)
            .map_err(|e| e.to_string())?;
    }

    bw.write_u32::<LittleEndian>(pixel_data_size)
        .map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(pixel_data_size)
        .map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(1).map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(3).map_err(|e| e.to_string())?;

    fs.seek(SeekFrom::Start(128)).map_err(|e| e.to_string())?;
    let mut take_stream = (&mut fs).take(pixel_data_size as u64);
    std::io::copy(&mut take_stream, &mut bw).map_err(|e| e.to_string())?;

    bw.write_u32::<LittleEndian>(1).map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(0).map_err(|e| e.to_string())?;

    bw.flush().map_err(|e| e.to_string())?;

    on_progress(&format!(
        "Successfully converted to {:?}",
        nif_path.file_name().unwrap_or_default()
    ));
    Ok(())
}

pub fn extract_nif_to_dds<F: Fn(&str)>(
    nif_path: &Path,
    dds_path: &Path,
    on_progress: F,
) -> Result<bool, String> {
    let f = File::open(nif_path).map_err(|e| e.to_string())?;
    let mut reader = BufReader::new(f);

    let mut header_str = String::new();
    loop {
        let b = reader.read_u8().map_err(|e| e.to_string())?;
        header_str.push(b as char);
        if b == b'\n' {
            break;
        }
        if header_str.len() > 120 {
            return Err("Not a valid Gamebryo header.".into());
        }
    }

    if !header_str.starts_with("Gamebryo File Format") {
        return Err("Not a Gamebryo NIF file.".into());
    }

    let _version = reader
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())?;
    let endian = reader.read_u8().map_err(|e| e.to_string())?;
    let is_le = endian != 0;

    let _user_version = read_u32_e(&mut reader, is_le)?;
    let num_blocks = read_u32_e(&mut reader, is_le)? as usize;
    let num_block_types = read_u16_e(&mut reader, is_le)? as usize;

    let mut block_types = Vec::with_capacity(num_block_types);
    for _ in 0..num_block_types {
        let len = read_u32_e(&mut reader, is_le)? as usize;
        let mut buf = vec![0u8; len];
        reader.read_exact(&mut buf).map_err(|e| e.to_string())?;
        block_types.push(String::from_utf8_lossy(&buf).into_owned());
    }

    let mut block_type_indices = Vec::with_capacity(num_blocks);
    for _ in 0..num_blocks {
        block_type_indices.push(read_u16_e(&mut reader, is_le)? as usize);
    }

    let mut block_sizes = Vec::with_capacity(num_blocks);
    for _ in 0..num_blocks {
        block_sizes.push(read_u32_e(&mut reader, is_le)? as u64);
    }

    let num_strings = read_u32_e(&mut reader, is_le)?;
    let _max_string_len = read_u32_e(&mut reader, is_le)?;
    for _ in 0..num_strings {
        let len = read_u32_e(&mut reader, is_le)? as i64;
        reader
            .seek(SeekFrom::Current(len))
            .map_err(|e| e.to_string())?;
    }

    let num_groups = read_u32_e(&mut reader, is_le)?;
    reader
        .seek(SeekFrom::Current(num_groups as i64 * 4))
        .map_err(|e| e.to_string())?;

    let blocks_data_start = reader.stream_position().map_err(|e| e.to_string())?;

    let mut target_block_idx = None;
    for (i, &type_idx) in block_type_indices.iter().enumerate() {
        if type_idx < block_types.len()
            && block_types[type_idx].contains("NiPersistentSrcTextureRendererData")
        {
            target_block_idx = Some(i);
            break;
        }
    }

    let target_idx = match target_block_idx {
        Some(idx) => idx,
        None => return Ok(false),
    };

    let mut block_offset = blocks_data_start;
    for size in block_sizes.iter().take(target_idx) {
        block_offset += size;
    }

    reader
        .seek(SeekFrom::Start(block_offset))
        .map_err(|e| e.to_string())?;

    let pixel_format = read_u32_e(&mut reader, is_le)?;
    let four_cc: &[u8; 4];
    let block_size: u32;

    if pixel_format == 4 {
        four_cc = b"DXT1";
        block_size = 8;
    } else if pixel_format == 5 {
        four_cc = b"DXT3";
        block_size = 16;
    } else if pixel_format == 6 {
        four_cc = b"DXT5";
        block_size = 16;
    } else {
        return Err(format!("Unsupported pixel format in NIF: {}", pixel_format));
    }

    reader
        .seek(SeekFrom::Current(59))
        .map_err(|e| e.to_string())?;

    let mip_map_count = read_u32_e(&mut reader, is_le)?;
    if mip_map_count > 24 {
        return Err(format!("Invalid mipmap count: {}", mip_map_count));
    }

    reader
        .seek(SeekFrom::Current(4))
        .map_err(|e| e.to_string())?;

    let mut width = 0u32;
    let mut height = 0u32;

    for i in 0..mip_map_count {
        let mip_w = read_u32_e(&mut reader, is_le)?;
        let mip_h = read_u32_e(&mut reader, is_le)?;
        let _mip_offset = read_u32_e(&mut reader, is_le)?;

        if i == 0 {
            width = mip_w;
            height = mip_h;
        }
    }

    let num_pixels = read_u32_e(&mut reader, is_le)? as u64;
    reader
        .seek(SeekFrom::Current(12))
        .map_err(|e| e.to_string())?;

    let out_fs = File::create(dds_path).map_err(|e| e.to_string())?;
    let mut writer = BufWriter::new(out_fs);

    writer.write_all(b"DDS ").map_err(|e| e.to_string())?;
    writer
        .write_u32::<LittleEndian>(124)
        .map_err(|e| e.to_string())?;

    let mut flags = 0x00000001 | 0x00000002 | 0x00000004 | 0x00001000;
    if mip_map_count > 1 {
        flags |= 0x00020000;
    }
    if block_size > 0 {
        flags |= 0x00080000;
    }

    writer
        .write_u32::<LittleEndian>(flags)
        .map_err(|e| e.to_string())?;
    writer
        .write_u32::<LittleEndian>(height)
        .map_err(|e| e.to_string())?;
    writer
        .write_u32::<LittleEndian>(width)
        .map_err(|e| e.to_string())?;

    let pitch_or_linear_size =
        std::cmp::max(1, width.div_ceil(4)) * std::cmp::max(1, height.div_ceil(4)) * block_size;
    writer
        .write_u32::<LittleEndian>(pitch_or_linear_size)
        .map_err(|e| e.to_string())?;
    writer
        .write_u32::<LittleEndian>(0)
        .map_err(|e| e.to_string())?;
    writer
        .write_u32::<LittleEndian>(std::cmp::max(1, mip_map_count))
        .map_err(|e| e.to_string())?;

    for _ in 0..11 {
        writer
            .write_u32::<LittleEndian>(0)
            .map_err(|e| e.to_string())?;
    }

    writer
        .write_u32::<LittleEndian>(32)
        .map_err(|e| e.to_string())?;
    writer
        .write_u32::<LittleEndian>(0x00000004)
        .map_err(|e| e.to_string())?;
    writer.write_all(four_cc).map_err(|e| e.to_string())?;
    for _ in 0..5 {
        writer
            .write_u32::<LittleEndian>(0)
            .map_err(|e| e.to_string())?;
    }

    let mut caps1 = 0x00001000;
    if mip_map_count > 1 {
        caps1 |= 0x00400008;
    }
    writer
        .write_u32::<LittleEndian>(caps1)
        .map_err(|e| e.to_string())?;
    for _ in 0..4 {
        writer
            .write_u32::<LittleEndian>(0)
            .map_err(|e| e.to_string())?;
    }

    let mut take_stream = (&mut reader).take(num_pixels);
    std::io::copy(&mut take_stream, &mut writer).map_err(|e| e.to_string())?;
    writer.flush().map_err(|e| e.to_string())?;

    on_progress(&format!(
        "Extracted {:?}",
        nif_path.file_name().unwrap_or_default()
    ));
    Ok(true)
}

fn read_u32_e<R: Read>(r: &mut R, is_le: bool) -> Result<u32, String> {
    if is_le {
        r.read_u32::<LittleEndian>().map_err(|e| e.to_string())
    } else {
        r.read_u32::<BigEndian>().map_err(|e| e.to_string())
    }
}

fn read_u16_e<R: Read>(r: &mut R, is_le: bool) -> Result<u16, String> {
    if is_le {
        r.read_u16::<LittleEndian>().map_err(|e| e.to_string())
    } else {
        r.read_u16::<BigEndian>().map_err(|e| e.to_string())
    }
}
