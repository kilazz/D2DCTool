use byteorder::{LittleEndian, ReadBytesExt, WriteBytesExt};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;

pub fn convert_dds_to_nif<F: Fn(&str)>(
    dds_path: &Path,
    nif_path: &Path,
    on_progress: F,
) -> Result<(), String> {
    let mut fs = File::open(dds_path).map_err(|e| e.to_string())?;
    let metadata = fs.metadata().map_err(|e| e.to_string())?;
    if metadata.len() < 128 {
        return Err("File is too small to be a DDS.".into());
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
    let mut bw = out_fs;

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
    let mut buffer = vec![0u8; 8192];
    let mut remaining = pixel_data_size as usize;
    while remaining > 0 {
        let to_read = std::cmp::min(buffer.len(), remaining);
        let read = fs.read(&mut buffer[..to_read]).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        bw.write_all(&buffer[..read]).map_err(|e| e.to_string())?;
        remaining -= read;
    }

    bw.write_u32::<LittleEndian>(1).map_err(|e| e.to_string())?;
    bw.write_u32::<LittleEndian>(0).map_err(|e| e.to_string())?;

    on_progress(&format!(
        "Successfully converted to {:?}",
        nif_path.file_name().unwrap()
    ));
    Ok(())
}

pub fn extract_nif_to_dds<F: Fn(&str)>(
    nif_path: &Path,
    dds_path: &Path,
    on_progress: F,
) -> Result<bool, String> {
    let nif_data = std::fs::read(nif_path).map_err(|e| e.to_string())?;

    let search_bytes = b"NiPersistentSrcTextureRendererData";
    let idx = index_of_bytes(&nif_data, search_bytes);
    if idx == -1 {
        return Ok(false);
    }

    let mut cursor = std::io::Cursor::new(&nif_data);
    cursor
        .seek(SeekFrom::Start((idx as usize + search_bytes.len()) as u64))
        .map_err(|e| e.to_string())?;

    // Skip BlockTypeIndex (2), BlockSize (4), NumStrings (4), MaxStringLen (4), Unknown (4) = 18
    cursor
        .seek(SeekFrom::Current(18))
        .map_err(|e| e.to_string())?;

    let pixel_format = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())?;
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

    // Skip: BitsPerPixel (1) + Unknown (4) + Unknown (4) + Flags (1) + Unknown (4) + Unknown (1) + Channels (40) + Palette (4) = 59
    cursor
        .seek(SeekFrom::Current(59))
        .map_err(|e| e.to_string())?;

    let mip_map_count = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())?;
    cursor
        .seek(SeekFrom::Current(4))
        .map_err(|e| e.to_string())?; // BytesPerPixel

    let mut width = 0u32;
    let mut height = 0u32;

    for i in 0..mip_map_count {
        let mip_w = cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?;
        let mip_h = cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?;
        let _mip_offset = cursor
            .read_u32::<LittleEndian>()
            .map_err(|e| e.to_string())?;

        if i == 0 {
            width = mip_w;
            height = mip_h;
        }
    }

    let num_pixels = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())?;
    cursor
        .seek(SeekFrom::Current(12))
        .map_err(|e| e.to_string())?;

    let mut data_size_to_read = num_pixels as usize;
    let current_pos = cursor.position() as usize;
    if current_pos + data_size_to_read > nif_data.len() {
        data_size_to_read = nif_data.len() - current_pos;
    }

    let raw_data = &nif_data[current_pos..current_pos + data_size_to_read];

    let out_fs = File::create(dds_path).map_err(|e| e.to_string())?;
    let mut writer = out_fs;

    writer.write_all(b"DDS ").map_err(|e| e.to_string())?;
    writer
        .write_u32::<LittleEndian>(124)
        .map_err(|e| e.to_string())?; // Size

    let mut flags = 0x00000001 | 0x00000002 | 0x00000004 | 0x00001000;
    if mip_map_count > 1 {
        flags |= 0x00020000; // DDSD_MIPMAPCOUNT
    }
    if block_size > 0 {
        flags |= 0x00080000; // DDSD_LINEARSIZE
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
        .map_err(|e| e.to_string())?; // Depth
    writer
        .write_u32::<LittleEndian>(std::cmp::max(1, mip_map_count))
        .map_err(|e| e.to_string())?; // MipMapCount

    for _ in 0..11 {
        writer
            .write_u32::<LittleEndian>(0)
            .map_err(|e| e.to_string())?; // Reserved1
    }

    // DDS_PIXELFORMAT
    writer
        .write_u32::<LittleEndian>(32)
        .map_err(|e| e.to_string())?; // Size
    writer
        .write_u32::<LittleEndian>(0x00000004)
        .map_err(|e| e.to_string())?; // Flags (DDPF_FOURCC)
    writer.write_all(four_cc).map_err(|e| e.to_string())?;
    writer
        .write_u32::<LittleEndian>(0)
        .map_err(|e| e.to_string())?; // RGBBitCount
    writer
        .write_u32::<LittleEndian>(0)
        .map_err(|e| e.to_string())?; // RBitMask
    writer
        .write_u32::<LittleEndian>(0)
        .map_err(|e| e.to_string())?; // GBitMask
    writer
        .write_u32::<LittleEndian>(0)
        .map_err(|e| e.to_string())?; // BBitMask
    writer
        .write_u32::<LittleEndian>(0)
        .map_err(|e| e.to_string())?; // ABitMask

    // DDS_HEADER_CAPS
    let mut caps1 = 0x00001000; // DDSCAPS_TEXTURE
    if mip_map_count > 1 {
        caps1 |= 0x00400008; // DDSCAPS_COMPLEX | DDSCAPS_MIPMAP
    }

    writer
        .write_u32::<LittleEndian>(caps1)
        .map_err(|e| e.to_string())?; // Caps
    writer
        .write_u32::<LittleEndian>(0)
        .map_err(|e| e.to_string())?; // Caps2
    writer
        .write_u32::<LittleEndian>(0)
        .map_err(|e| e.to_string())?; // Caps3
    writer
        .write_u32::<LittleEndian>(0)
        .map_err(|e| e.to_string())?; // Caps4
    writer
        .write_u32::<LittleEndian>(0)
        .map_err(|e| e.to_string())?; // Reserved2

    writer.write_all(raw_data).map_err(|e| e.to_string())?;

    on_progress(&format!("Extracted {:?}", nif_path.file_name().unwrap()));
    Ok(true)
}

fn index_of_bytes(array: &[u8], pattern: &[u8]) -> i32 {
    if pattern.len() > array.len() {
        return -1;
    }
    for i in 0..=array.len() - pattern.len() {
        if &array[i..i + pattern.len()] == pattern {
            return i as i32;
        }
    }
    -1
}
