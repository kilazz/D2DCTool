use byteorder::{BigEndian, LittleEndian, ReadBytesExt, WriteBytesExt};
use quick_xml::Reader;
use quick_xml::events::Event;
use std::fs::File;
use std::io::{Cursor, Read, Seek, SeekFrom, Write};
use std::path::Path;

pub struct GamebryoNode {
    pub data: u32,
    pub value: Option<String>,
    pub attributes: Vec<GamebryoAttribute>,
    pub children: Vec<GamebryoNode>,
}

pub struct GamebryoAttribute {
    pub data: u32,
    pub value: String,
}

pub fn get_hash_value(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let mut hash: u64 = 0;
    for c in text.bytes() {
        hash = (hash * 0x21 + c as u64) % 0xFFFFFFFF;
    }
    hash as u32
}

pub fn to_xml_string(
    node: &GamebryoNode,
    hashes: &std::collections::HashMap<u32, String>,
    indent: usize,
) -> String {
    let ind = "  ".repeat(indent);
    let mut xml = format!("{}<Node", ind);
    if let Some(name) = hashes.get(&node.data) {
        xml.push_str(&format!(" Name=\"{}\"", name));
    } else {
        xml.push_str(&format!(" Data=\"{:08X}\"", node.data));
    }
    xml.push_str(">\n");

    if let Some(ref val) = node.value {
        xml.push_str(&format!("{}  <Value>{}</Value>\n", ind, escape_xml(val)));
    }

    if !node.attributes.is_empty() {
        xml.push_str(&format!("{}  <Attributes>\n", ind));
        for attr in &node.attributes {
            let mut attr_xml = format!("{}    <Attr Value=\"{}\"", ind, escape_xml(&attr.value));
            if let Some(name) = hashes.get(&attr.data) {
                attr_xml.push_str(&format!(" Name=\"{}\"", name));
            } else {
                attr_xml.push_str(&format!(" Data=\"{:08X}\"", attr.data));
            }
            attr_xml.push_str("/>\n");
            xml.push_str(&attr_xml);
        }
        xml.push_str(&format!("{}  </Attributes>\n", ind));
    }

    if !node.children.is_empty() {
        xml.push_str(&format!("{}  <Children>\n", ind));
        for child in &node.children {
            xml.push_str(&to_xml_string(child, hashes, indent + 2));
        }
        xml.push_str(&format!("{}  </Children>\n", ind));
    }

    xml.push_str(&format!("{}</Node>\n", ind));
    xml
}

fn escape_xml(text: &str) -> String {
    let mut s = String::new();
    for c in text.chars() {
        match c {
            '<' => s.push_str("&lt;"),
            '>' => s.push_str("&gt;"),
            '&' => s.push_str("&amp;"),
            '"' => s.push_str("&quot;"),
            '\'' => s.push_str("&apos;"),
            _ => s.push(c),
        }
    }
    s
}

pub fn parse_xml(xml_content: &str) -> Result<GamebryoNode, String> {
    let mut reader = Reader::from_str(xml_content);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();
    let mut node_stack: Vec<GamebryoNode> = Vec::new();
    let mut current_attr_list: Option<Vec<GamebryoAttribute>> = None;
    let mut in_value = false;

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(ref e)) => {
                let name_bytes = e.name();
                let name = reader
                    .decoder()
                    .decode(name_bytes.as_ref())
                    .map_err(|e| e.to_string())?;
                match name.as_ref() {
                    "Node" => {
                        let mut data = 0;
                        for attr in e.attributes() {
                            let attr = attr.map_err(|e| e.to_string())?;
                            let key = reader
                                .decoder()
                                .decode(attr.key.as_ref())
                                .map_err(|e| e.to_string())?;
                            let val = reader
                                .decoder()
                                .decode(attr.value.as_ref())
                                .map_err(|e| e.to_string())?;
                            if key == "Name" {
                                data = get_hash_value(&val);
                            } else if key == "Data" {
                                data = u32::from_str_radix(val.trim_start_matches("0x"), 16)
                                    .unwrap_or_else(|_| val.parse().unwrap_or(0));
                            }
                        }
                        node_stack.push(GamebryoNode {
                            data,
                            value: None,
                            attributes: Vec::new(),
                            children: Vec::new(),
                        });
                    }
                    "Attributes" => {
                        current_attr_list = Some(Vec::new());
                    }
                    "Children" => {}
                    "Value" => {
                        in_value = true;
                    }
                    _ => {}
                }
            }
            Ok(Event::Empty(ref e)) => {
                let name_bytes = e.name();
                let name = reader
                    .decoder()
                    .decode(name_bytes.as_ref())
                    .map_err(|e| e.to_string())?;
                if name == "Attr" {
                    let mut data = 0;
                    let mut value = String::new();
                    for attr in e.attributes() {
                        let attr = attr.map_err(|e| e.to_string())?;
                        let key = reader
                            .decoder()
                            .decode(attr.key.as_ref())
                            .map_err(|e| e.to_string())?;
                        let val = reader
                            .decoder()
                            .decode(attr.value.as_ref())
                            .map_err(|e| e.to_string())?;
                        if key == "Value" {
                            value = val.into_owned();
                        } else if key == "Name" {
                            data = get_hash_value(&val);
                        } else if key == "Data" {
                            data = u32::from_str_radix(val.trim_start_matches("0x"), 16)
                                .unwrap_or_else(|_| val.parse().unwrap_or(0));
                        }
                    }
                    if let Some(ref mut attrs) = current_attr_list {
                        attrs.push(GamebryoAttribute { data, value });
                    }
                }
            }
            Ok(Event::Text(ref e)) => {
                let txt = reader
                    .decoder()
                    .decode(e.as_ref())
                    .map_err(|e| e.to_string())?
                    .into_owned();
                if in_value && let Some(node) = node_stack.last_mut() {
                    node.value = Some(txt);
                }
            }
            Ok(Event::End(ref e)) => {
                let name_bytes = e.name();
                let name = reader
                    .decoder()
                    .decode(name_bytes.as_ref())
                    .map_err(|e| e.to_string())?;
                match name.as_ref() {
                    "Node" => {
                        if node_stack.len() > 1 {
                            let child = node_stack.pop().unwrap();
                            node_stack.last_mut().unwrap().children.push(child);
                        }
                    }
                    "Attributes" => {
                        if let Some(attrs) = current_attr_list.take()
                            && let Some(node) = node_stack.last_mut()
                        {
                            node.attributes = attrs;
                        }
                    }
                    "Value" => {
                        in_value = false;
                    }
                    _ => {}
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => return Err(e.to_string()),
            _ => {}
        }
        buf.clear();
    }

    node_stack
        .pop()
        .ok_or_else(|| "Empty XML DOM stack".to_string())
}

pub fn extract_binary_xml_to_real_xml(
    nif_path: &Path,
    xml_path: &Path,
    hashes: &std::collections::HashMap<u32, String>,
) -> Result<(), String> {
    let mut f = File::open(nif_path).map_err(|e| e.to_string())?;
    let mut data = Vec::new();
    f.read_to_end(&mut data).map_err(|e| e.to_string())?;

    let mut cursor = Cursor::new(&data);

    let mut header_str = String::new();
    while cursor.position() < data.len() as u64 {
        let b = cursor.read_u8().map_err(|e| e.to_string())?;
        header_str.push(b as char);
        if b == b'\n' {
            break;
        }
    }

    if !header_str.starts_with("Gamebryo File Format") {
        return Err("Not a valid Gamebryo file.".into());
    }

    let _version = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())?;
    let endian = cursor.read_u8().map_err(|e| e.to_string())?;
    let is_little_endian = endian != 0;

    let _user_version = read_u32_e(&mut cursor, is_little_endian)?;
    let num_blocks = read_u32_e(&mut cursor, is_little_endian)?;
    let num_block_types = read_u16_e(&mut cursor, is_little_endian)?;

    let mut block_types = Vec::new();
    for _ in 0..num_block_types {
        let len = read_u32_e(&mut cursor, is_little_endian)? as usize;
        let mut buf = vec![0u8; len];
        cursor.read_exact(&mut buf).map_err(|e| e.to_string())?;
        block_types.push(String::from_utf8_lossy(&buf).into_owned());
    }

    let mut block_type_indices = Vec::new();
    for _ in 0..num_blocks {
        block_type_indices.push(read_u16_e(&mut cursor, is_little_endian)?);
    }

    let mut block_sizes = Vec::new();
    for _ in 0..num_blocks {
        block_sizes.push(read_u32_e(&mut cursor, is_little_endian)?);
    }

    let num_strings = read_u32_e(&mut cursor, is_little_endian)?;
    let _max_string_len = read_u32_e(&mut cursor, is_little_endian)?;

    for _ in 0..num_strings {
        let len = read_u32_e(&mut cursor, is_little_endian)? as u64;
        cursor
            .seek(SeekFrom::Current(len as i64))
            .map_err(|e| e.to_string())?;
    }

    let num_groups = read_u32_e(&mut cursor, is_little_endian)?;
    for _ in 0..num_groups {
        cursor
            .seek(SeekFrom::Current(4))
            .map_err(|e| e.to_string())?;
    }

    if num_blocks != 1
        || !block_types[block_type_indices[0] as usize].contains("xml::dom::CStreamableNode")
    {
        return Err("File does not contain a valid xml::dom::CStreamableNode block.".into());
    }

    let _node_count = read_u32_e(&mut cursor, is_little_endian)?;
    let _total_attr_count = read_u32_e(&mut cursor, is_little_endian)?;
    let string_block_size = read_u32_e(&mut cursor, is_little_endian)? as usize;

    let mut string_block = vec![0u8; string_block_size];
    cursor
        .read_exact(&mut string_block)
        .map_err(|e| e.to_string())?;
    let mut str_offset = 0;

    let mut get_next_string = move |str_offset: &mut usize| -> String {
        let mut end = *str_offset;
        while end < string_block.len() && string_block[end] != 0 {
            end += 1;
        }
        let s = String::from_utf8_lossy(&string_block[*str_offset..end]).into_owned();
        *str_offset = end + 1;
        let mut sb = String::with_capacity(s.len());
        for c in s.chars() {
            let is_xml_char = c == '\t'
                || c == '\n'
                || c == '\r'
                || ('\u{0020}'..='\u{D7FF}').contains(&c)
                || ('\u{E000}'..='\u{FFFD}').contains(&c);
            if is_xml_char {
                sb.push(c);
            }
        }
        sb
    };

    fn read_node<R: Read>(
        r: &mut R,
        is_le: bool,
        str_offset: &mut usize,
        get_next_string: &mut dyn FnMut(&mut usize) -> String,
    ) -> Result<GamebryoNode, String> {
        let flags = r.read_u8().map_err(|e| e.to_string())?;
        let data = if is_le {
            r.read_u32::<LittleEndian>().map_err(|e| e.to_string())?
        } else {
            r.read_u32::<BigEndian>().map_err(|e| e.to_string())?
        };

        let mut value = None;
        if (flags & 0x04) != 0 {
            value = Some(get_next_string(str_offset));
        }

        let mut attributes = Vec::new();
        if (flags & 0x02) != 0 {
            let attr_count = if (flags & 0x08) == 0 {
                if is_le {
                    r.read_u32::<LittleEndian>().map_err(|e| e.to_string())?
                } else {
                    r.read_u32::<BigEndian>().map_err(|e| e.to_string())?
                }
            } else {
                r.read_u8().map_err(|e| e.to_string())? as u32
            };

            for _ in 0..attr_count {
                let attr_data = if is_le {
                    r.read_u32::<LittleEndian>().map_err(|e| e.to_string())?
                } else {
                    r.read_u32::<BigEndian>().map_err(|e| e.to_string())?
                };
                let attr_value = get_next_string(str_offset);
                attributes.push(GamebryoAttribute {
                    data: attr_data,
                    value: attr_value,
                });
            }
        }

        let mut children = Vec::new();
        if (flags & 0x01) != 0 {
            let child_count = if (flags & 0x08) == 0 {
                if is_le {
                    r.read_u32::<LittleEndian>().map_err(|e| e.to_string())?
                } else {
                    r.read_u32::<BigEndian>().map_err(|e| e.to_string())?
                }
            } else {
                r.read_u8().map_err(|e| e.to_string())? as u32
            };

            for _ in 0..child_count {
                children.push(read_node(r, is_le, str_offset, get_next_string)?);
            }
        }

        Ok(GamebryoNode {
            data,
            value,
            attributes,
            children,
        })
    }

    let root_node = read_node(
        &mut cursor,
        is_little_endian,
        &mut str_offset,
        &mut get_next_string,
    )?;

    let xml_str = to_xml_string(&root_node, hashes, 0);
    let mut out_file = File::create(xml_path).map_err(|e| e.to_string())?;
    out_file
        .write_all(b"<?xml version=\"1.0\" encoding=\"utf-8\"?>\n")
        .map_err(|e| e.to_string())?;
    out_file
        .write_all(xml_str.as_bytes())
        .map_err(|e| e.to_string())?;

    Ok(())
}

pub fn repack_real_xml_to_binary_xml(
    original_nif_path: &Path,
    xml_path: &Path,
    out_nif_path: &Path,
) -> Result<(), String> {
    let xml_data = std::fs::read_to_string(xml_path).map_err(|e| e.to_string())?;
    let root_node = parse_xml(&xml_data)?;

    let original_data = std::fs::read(original_nif_path).map_err(|e| e.to_string())?;
    let mut cursor = Cursor::new(&original_data);

    let mut header_str = String::new();
    while cursor.position() < original_data.len() as u64 {
        let b = cursor.read_u8().map_err(|e| e.to_string())?;
        header_str.push(b as char);
        if b == b'\n' {
            break;
        }
    }

    if !header_str.starts_with("Gamebryo File Format") {
        return Err("Original file is not a valid Gamebryo file.".into());
    }

    let _version = cursor
        .read_u32::<LittleEndian>()
        .map_err(|e| e.to_string())?;
    let endian = cursor.read_u8().map_err(|e| e.to_string())?;
    let is_little_endian = endian != 0;

    let _user_version = read_u32_e(&mut cursor, is_little_endian)?;
    let num_blocks = read_u32_e(&mut cursor, is_little_endian)?;
    let num_block_types = read_u16_e(&mut cursor, is_little_endian)?;

    for _ in 0..num_block_types {
        let len = read_u32_e(&mut cursor, is_little_endian)? as u64;
        cursor
            .seek(SeekFrom::Current(len as i64))
            .map_err(|e| e.to_string())?;
    }

    for _ in 0..num_blocks {
        cursor
            .seek(SeekFrom::Current(2))
            .map_err(|e| e.to_string())?;
    }

    let block_size_offset = cursor.position() as usize;
    let mut block_sizes = Vec::new();
    for _ in 0..num_blocks {
        block_sizes.push(read_u32_e(&mut cursor, is_little_endian)?);
    }

    let num_strings = read_u32_e(&mut cursor, is_little_endian)?;
    let _max_string_len = read_u32_e(&mut cursor, is_little_endian)?;

    for _ in 0..num_strings {
        let len = read_u32_e(&mut cursor, is_little_endian)? as u64;
        cursor
            .seek(SeekFrom::Current(len as i64))
            .map_err(|e| e.to_string())?;
    }

    let num_groups = read_u32_e(&mut cursor, is_little_endian)?;
    for _ in 0..num_groups {
        cursor
            .seek(SeekFrom::Current(4))
            .map_err(|e| e.to_string())?;
    }

    let after_header_offset = cursor.position() as usize;

    let mut total_nodes = 0u32;
    let mut total_attrs = 0u32;
    let mut string_block = Vec::new();
    let mut node_data_block = Vec::new();

    fn write_node(
        node: &GamebryoNode,
        is_le: bool,
        total_nodes: &mut u32,
        total_attrs: &mut u32,
        string_block: &mut Vec<u8>,
        node_data: &mut Vec<u8>,
    ) -> Result<(), String> {
        *total_nodes += 1;
        let mut flags = 0u8;
        if !node.children.is_empty() {
            flags |= 0x01;
        }
        if !node.attributes.is_empty() {
            flags |= 0x02;
        }
        if node.value.is_some() {
            flags |= 0x04;
        }

        let compress = node.children.len() <= 255 && node.attributes.len() <= 255;
        if compress {
            flags |= 0x08;
        }

        node_data.write_u8(flags).map_err(|e| e.to_string())?;
        if is_le {
            node_data
                .write_u32::<LittleEndian>(node.data)
                .map_err(|e| e.to_string())?;
        } else {
            node_data
                .write_u32::<BigEndian>(node.data)
                .map_err(|e| e.to_string())?;
        }

        if let Some(ref val) = node.value {
            string_block.extend_from_slice(val.as_bytes());
            string_block.push(0);
        }

        if !node.attributes.is_empty() {
            if compress {
                node_data
                    .write_u8(node.attributes.len() as u8)
                    .map_err(|e| e.to_string())?;
            } else {
                if is_le {
                    node_data
                        .write_u32::<LittleEndian>(node.attributes.len() as u32)
                        .map_err(|e| e.to_string())?;
                } else {
                    node_data
                        .write_u32::<BigEndian>(node.attributes.len() as u32)
                        .map_err(|e| e.to_string())?;
                }
            }

            for attr in &node.attributes {
                *total_attrs += 1;
                if is_le {
                    node_data
                        .write_u32::<LittleEndian>(attr.data)
                        .map_err(|e| e.to_string())?;
                } else {
                    node_data
                        .write_u32::<BigEndian>(attr.data)
                        .map_err(|e| e.to_string())?;
                }
                string_block.extend_from_slice(attr.value.as_bytes());
                string_block.push(0);
            }
        }

        if !node.children.is_empty() {
            if compress {
                node_data
                    .write_u8(node.children.len() as u8)
                    .map_err(|e| e.to_string())?;
            } else {
                if is_le {
                    node_data
                        .write_u32::<LittleEndian>(node.children.len() as u32)
                        .map_err(|e| e.to_string())?;
                } else {
                    node_data
                        .write_u32::<BigEndian>(node.children.len() as u32)
                        .map_err(|e| e.to_string())?;
                }
            }

            for child in &node.children {
                write_node(
                    child,
                    is_le,
                    total_nodes,
                    total_attrs,
                    string_block,
                    node_data,
                )?;
            }
        }

        Ok(())
    }

    write_node(
        &root_node,
        is_little_endian,
        &mut total_nodes,
        &mut total_attrs,
        &mut string_block,
        &mut node_data_block,
    )?;

    let new_block_size = 12 + string_block.len() as u32 + node_data_block.len() as u32;

    let mut out_file = File::create(out_nif_path).map_err(|e| e.to_string())?;
    out_file
        .write_all(&original_data[0..block_size_offset])
        .map_err(|e| e.to_string())?;
    write_u32_e(&mut out_file, new_block_size, is_little_endian)?;

    let after_block_sizes_offset = block_size_offset + 4;
    let len_to_copy = after_header_offset - after_block_sizes_offset;
    out_file
        .write_all(&original_data[after_block_sizes_offset..after_block_sizes_offset + len_to_copy])
        .map_err(|e| e.to_string())?;

    write_u32_e(&mut out_file, total_nodes, is_little_endian)?;
    write_u32_e(&mut out_file, total_attrs, is_little_endian)?;
    write_u32_e(&mut out_file, string_block.len() as u32, is_little_endian)?;
    out_file
        .write_all(&string_block)
        .map_err(|e| e.to_string())?;
    out_file
        .write_all(&node_data_block)
        .map_err(|e| e.to_string())?;

    let mut footer_offset = after_header_offset as u64;
    for size in block_sizes {
        footer_offset += size as u64;
    }
    if footer_offset < original_data.len() as u64 {
        out_file
            .write_all(&original_data[footer_offset as usize..])
            .map_err(|e| e.to_string())?;
    }

    Ok(())
}

pub fn read_u32_e<R: Read>(r: &mut R, is_le: bool) -> Result<u32, String> {
    if is_le {
        r.read_u32::<LittleEndian>().map_err(|e| e.to_string())
    } else {
        r.read_u32::<BigEndian>().map_err(|e| e.to_string())
    }
}

pub fn read_u16_e<R: Read>(r: &mut R, is_le: bool) -> Result<u16, String> {
    if is_le {
        r.read_u16::<LittleEndian>().map_err(|e| e.to_string())
    } else {
        r.read_u16::<BigEndian>().map_err(|e| e.to_string())
    }
}

pub fn write_u32_e<W: Write>(w: &mut W, val: u32, is_le: bool) -> Result<(), String> {
    if is_le {
        w.write_u32::<LittleEndian>(val).map_err(|e| e.to_string())
    } else {
        w.write_u32::<BigEndian>(val).map_err(|e| e.to_string())
    }
}
