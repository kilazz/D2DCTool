use byteorder::{BigEndian, LittleEndian, ReadBytesExt};
use regex::Regex;
use std::collections::HashSet;
use std::fs::File;
use std::io::{BufRead, BufReader, Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;
use walkdir::WalkDir;

pub fn scan_and_verify<F: Fn(&str)>(xml_dir: &Path, log: F) -> Result<(), String> {
    let dict_file = "hash_dictionary.txt";
    let out_file = "unknown_hashes.txt";

    let mut known_strings = HashSet::new();
    if Path::new(dict_file).exists() {
        let f = File::open(dict_file).map_err(|e| e.to_string())?;
        let r = BufReader::new(f);
        for line in r.lines().map_while(Result::ok) {
            let trimmed = line.trim().to_string();
            if !trimmed.is_empty() {
                known_strings.insert(trimmed);
            }
        }
    }

    log(&format!(
        "Loaded {} known strings from dictionary.",
        known_strings.len()
    ));

    let mut known_hashes = HashSet::new();
    for s in &known_strings {
        known_hashes.insert(super::xml_converter::get_hash_value(s));
    }

    let mut all_found_hashes = HashSet::new();
    let mut binary_count = 0;
    let mut text_count = 0;

    let mut files = Vec::new();
    for entry in WalkDir::new(xml_dir).into_iter().filter_map(|e| e.ok()) {
        if entry.path().is_file()
            && entry.path().extension().and_then(|s| s.to_str()) == Some("xml")
        {
            files.push(entry.path().to_path_buf());
        }
    }

    log(&format!("Scanning {} XML files...", files.len()));

    let text_regex = Regex::new(r#"\b(?:Name|Data)="([0-9A-Fa-f]{8})""#).unwrap();

    for (i, file) in files.iter().enumerate() {
        if i > 0 && i % 500 == 0 {
            log(&format!("Scanning... {}/{}", i, files.len()));
        }

        if let Some(hashes) = parse_binary_xml(file) {
            for h in hashes {
                all_found_hashes.insert(h);
            }
            binary_count += 1;
        } else {
            let hashes = parse_text_xml(file, &text_regex);
            for h in hashes {
                all_found_hashes.insert(h);
            }
            text_count += 1;
        }
    }

    log(&format!(
        "Processed binary XMLs: {}, text XMLs: {}",
        binary_count, text_count
    ));
    log(&format!(
        "Total unique hashes found: {}",
        all_found_hashes.len()
    ));

    let mut unknown_hashes: HashSet<u32> = all_found_hashes.clone();
    for kh in &known_hashes {
        unknown_hashes.remove(kh);
    }

    let known_found = all_found_hashes.len() - unknown_hashes.len();
    log(&format!("Already KNOWN (in dictionary): {}", known_found));
    log(&format!("Remaining UNKNOWN: {}", unknown_hashes.len()));

    let mut sorted_strings: Vec<String> = known_strings.into_iter().collect();
    sorted_strings.sort_by_key(|a| a.to_lowercase());
    let mut dict_out = File::create(dict_file).map_err(|e| e.to_string())?;
    for s in &sorted_strings {
        writeln!(dict_out, "{}", s).map_err(|e| e.to_string())?;
    }

    let mut sorted_unknowns: Vec<u32> = unknown_hashes.into_iter().collect();
    sorted_unknowns.sort();
    let mut sw = File::create(out_file).map_err(|e| e.to_string())?;
    for h in sorted_unknowns {
        writeln!(sw, "{:08X}", h).map_err(|e| e.to_string())?;
    }

    log("[OK] Cleaned dictionary and unknown hashes saved.");
    Ok(())
}

fn parse_binary_xml(filepath: &Path) -> Option<HashSet<u32>> {
    let mut f = File::open(filepath).ok()?;
    let mut data = Vec::new();
    f.read_to_end(&mut data).ok()?;

    let mut cursor = Cursor::new(&data);

    let mut header_str = String::new();
    while cursor.position() < data.len() as u64 {
        let b = cursor.read_u8().ok()?;
        header_str.push(b as char);
        if b == b'\n' {
            break;
        }
    }
    if !header_str.starts_with("Gamebryo File Format") {
        return None;
    }

    let _version = cursor.read_u32::<LittleEndian>().ok()?;
    let endian = cursor.read_u8().ok()?;
    let is_little_endian = endian != 0;

    let _user_version = read_u32_opt(&mut cursor, is_little_endian)?;
    let num_blocks = read_u32_opt(&mut cursor, is_little_endian)?;
    let num_block_types = read_u16_opt(&mut cursor, is_little_endian)?;

    let mut block_types = Vec::new();
    for _ in 0..num_block_types {
        let len = read_u32_opt(&mut cursor, is_little_endian)? as usize;
        let mut buf = vec![0u8; len];
        cursor.read_exact(&mut buf).ok()?;
        block_types.push(String::from_utf8_lossy(&buf).into_owned());
    }

    let mut block_type_indices = Vec::new();
    for _ in 0..num_blocks {
        block_type_indices.push(read_u16_opt(&mut cursor, is_little_endian)?);
    }

    let mut block_sizes = Vec::new();
    for _ in 0..num_blocks {
        block_sizes.push(read_u32_opt(&mut cursor, is_little_endian)?);
    }

    let num_strings = read_u32_opt(&mut cursor, is_little_endian)?;
    let _max_string_len = read_u32_opt(&mut cursor, is_little_endian)?;
    for _ in 0..num_strings {
        let len = read_u32_opt(&mut cursor, is_little_endian)? as u64;
        cursor.seek(SeekFrom::Current(len as i64)).ok()?;
    }

    let num_groups = read_u32_opt(&mut cursor, is_little_endian)?;
    for _ in 0..num_groups {
        cursor.seek(SeekFrom::Current(4)).ok()?;
    }

    if num_blocks != 1
        || !block_types[block_type_indices[0] as usize].contains("xml::dom::CStreamableNode")
    {
        return Some(HashSet::new());
    }

    let _node_count = read_u32_opt(&mut cursor, is_little_endian)?;
    let _total_attr_count = read_u32_opt(&mut cursor, is_little_endian)?;
    let string_block_size = read_u32_opt(&mut cursor, is_little_endian)? as u64;
    cursor
        .seek(SeekFrom::Current(string_block_size as i64))
        .ok()?;

    let mut hashes = HashSet::new();

    fn read_node<R: Read>(r: &mut R, is_le: bool, hashes: &mut HashSet<u32>) -> Option<()> {
        let flags = r.read_u8().ok()?;
        let data = if is_le {
            r.read_u32::<LittleEndian>().ok()?
        } else {
            r.read_u32::<BigEndian>().ok()?
        };
        hashes.insert(data);

        if (flags & 0x02) != 0 {
            let attr_count = if (flags & 0x08) == 0 {
                if is_le {
                    r.read_u32::<LittleEndian>().ok()?
                } else {
                    r.read_u32::<BigEndian>().ok()?
                }
            } else {
                r.read_u8().ok()? as u32
            };

            for _ in 0..attr_count {
                let attr_data = if is_le {
                    r.read_u32::<LittleEndian>().ok()?
                } else {
                    r.read_u32::<BigEndian>().ok()?
                };
                hashes.insert(attr_data);
            }
        }

        if (flags & 0x01) != 0 {
            let child_count = if (flags & 0x08) == 0 {
                if is_le {
                    r.read_u32::<LittleEndian>().ok()?
                } else {
                    r.read_u32::<BigEndian>().ok()?
                }
            } else {
                r.read_u8().ok()? as u32
            };

            for _ in 0..child_count {
                read_node(r, is_le, hashes)?;
            }
        }

        Some(())
    }

    read_node(&mut cursor, is_little_endian, &mut hashes)?;
    Some(hashes)
}

fn parse_text_xml(filepath: &Path, regex: &Regex) -> HashSet<u32> {
    let mut hashes = HashSet::new();
    if let Ok(content) = std::fs::read_to_string(filepath) {
        for cap in regex.captures_iter(&content) {
            if let Some(m) = cap.get(1)
                && let Ok(h) = u32::from_str_radix(m.as_str(), 16)
            {
                hashes.insert(h);
            }
        }
    }
    hashes
}

fn read_u32_opt<R: Read>(r: &mut R, is_le: bool) -> Option<u32> {
    if is_le {
        r.read_u32::<LittleEndian>().ok()
    } else {
        r.read_u32::<BigEndian>().ok()
    }
}

fn read_u16_opt<R: Read>(r: &mut R, is_le: bool) -> Option<u16> {
    if is_le {
        r.read_u16::<LittleEndian>().ok()
    } else {
        r.read_u16::<BigEndian>().ok()
    }
}
