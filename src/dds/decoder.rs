pub fn decode_dxt_to_rgba(width: u32, height: u32, data: &[u8], four_cc: &[u8; 4]) -> Vec<u8> {
    let mut rgba = vec![0u8; (width * height * 4) as usize];
    let block_count_x = width.div_ceil(4);
    let block_count_y = height.div_ceil(4);
    let is_dxt1 = four_cc == b"DXT1";
    let is_dxt3 = four_cc == b"DXT3";
    let block_size = if is_dxt1 { 8 } else { 16 };

    let mut src_offset = 0;
    for by in 0..block_count_y {
        for bx in 0..block_count_x {
            if src_offset + block_size > data.len() {
                return rgba;
            }
            let block = &data[src_offset..src_offset + block_size];
            src_offset += block_size;

            decode_block(
                (bx * 4, by * 4),
                (width, height),
                block,
                is_dxt1,
                is_dxt3,
                &mut rgba,
            );
        }
    }
    rgba
}

fn decode_block(
    pos: (u32, u32),
    dims: (u32, u32),
    block: &[u8],
    is_dxt1: bool,
    is_dxt3: bool,
    out: &mut [u8],
) {
    let (start_x, start_y) = pos;
    let (img_w, img_h) = dims;
    let color_offset = if is_dxt1 { 0 } else { 8 };
    let c0_raw = u16::from_le_bytes([block[color_offset], block[color_offset + 1]]);
    let c1_raw = u16::from_le_bytes([block[color_offset + 2], block[color_offset + 3]]);

    let c0 = rgb565_to_rgb888(c0_raw);
    let c1 = rgb565_to_rgb888(c1_raw);

    let mut palette = [[0u8; 4]; 4];
    palette[0] = [c0[0], c0[1], c0[2], 255];
    palette[1] = [c1[0], c1[1], c1[2], 255];

    if is_dxt1 && c0_raw <= c1_raw {
        palette[2] = [
            ((c0[0] as u16 + c1[0] as u16) / 2) as u8,
            ((c0[1] as u16 + c1[1] as u16) / 2) as u8,
            ((c0[2] as u16 + c1[2] as u16) / 2) as u8,
            255,
        ];
        palette[3] = [0, 0, 0, 0];
    } else {
        palette[2] = [
            ((2 * c0[0] as u16 + c1[0] as u16) / 3) as u8,
            ((2 * c0[1] as u16 + c1[1] as u16) / 3) as u8,
            ((2 * c0[2] as u16 + c1[2] as u16) / 3) as u8,
            255,
        ];
        palette[3] = [
            ((c0[0] as u16 + 2 * c1[0] as u16) / 3) as u8,
            ((c0[1] as u16 + 2 * c1[1] as u16) / 3) as u8,
            ((c0[2] as u16 + 2 * c1[2] as u16) / 3) as u8,
            255,
        ];
    }

    let code_table = u32::from_le_bytes([
        block[color_offset + 4],
        block[color_offset + 5],
        block[color_offset + 6],
        block[color_offset + 7],
    ]);

    for y in 0..4 {
        for x in 0..4 {
            let px = start_x + x;
            let py = start_y + y;
            if px >= img_w || py >= img_h {
                continue;
            }

            let shift = (y * 4 + x) * 2;
            let code = ((code_table >> shift) & 0x03) as usize;
            let mut pixel = palette[code];

            if !is_dxt1 {
                if is_dxt3 {
                    let a_byte = block[(y * 2 + x / 2) as usize];
                    let a_nibble = if x % 2 == 0 {
                        a_byte & 0x0F
                    } else {
                        a_byte >> 4
                    };
                    pixel[3] = a_nibble * 17;
                } else {
                    // DXT5
                    let a0 = block[0];
                    let a1 = block[1];
                    let mut a_pal = [0u8; 8];
                    a_pal[0] = a0;
                    a_pal[1] = a1;
                    if a0 > a1 {
                        for (step, item) in a_pal[2..8].iter_mut().enumerate() {
                            let i = step + 2;
                            *item = (((8 - i) as u16 * a0 as u16 + (i - 1) as u16 * a1 as u16) / 7)
                                as u8;
                        }
                    } else {
                        for (step, item) in a_pal[2..6].iter_mut().enumerate() {
                            let i = step + 2;
                            *item = (((6 - i) as u16 * a0 as u16 + (i - 1) as u16 * a1 as u16) / 5)
                                as u8;
                        }
                        a_pal[6] = 0;
                        a_pal[7] = 255;
                    }
                    let bit_offset = (y * 4 + x) * 3;
                    let byte_idx = 2 + bit_offset / 8;
                    let sub_shift = bit_offset % 8;
                    let three_bytes = u32::from_le_bytes([
                        block[byte_idx as usize],
                        block.get(byte_idx as usize + 1).copied().unwrap_or(0),
                        block.get(byte_idx as usize + 2).copied().unwrap_or(0),
                        0,
                    ]);
                    let a_idx = ((three_bytes >> sub_shift) & 0x07) as usize;
                    pixel[3] = a_pal[a_idx];
                }
            }

            let idx = ((py * img_w + px) * 4) as usize;
            out[idx..idx + 4].copy_from_slice(&pixel);
        }
    }
}

fn rgb565_to_rgb888(val: u16) -> [u8; 3] {
    let r = ((val >> 11) & 0x1F) as u8;
    let g = ((val >> 5) & 0x3F) as u8;
    let b = (val & 0x1F) as u8;
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}
