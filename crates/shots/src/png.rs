//! A picture as a PNG file, written without compressing it: the format's own "stored" blocks.
//! Large files, read by everything, and no code to trust but these few lines.

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for &byte in bytes {
        crc ^= byte as u32;
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & (crc & 1).wrapping_neg());
        }
    }
    !crc
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in bytes.chunks(5552) {
        for &byte in chunk {
            a += byte as u32;
            b += a;
        }
        (a, b) = (a % 65521, b % 65521);
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend((data.len() as u32).to_be_bytes());
    let from = out.len();
    out.extend(kind);
    out.extend(data);
    let crc = crc32(&out[from..]);
    out.extend(crc.to_be_bytes());
}

/// The file of a picture of `width` by `height` pixels, four bytes a pixel: red, green, blue
/// and alpha, the rows from the top.
pub fn png(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    // The rows, each after the byte that says it is not filtered.
    let row = width as usize * 4;
    let mut raw = Vec::with_capacity((row + 1) * height as usize);
    for line in rgba.chunks(row).take(height as usize) {
        raw.push(0);
        raw.extend(line);
    }
    // A zlib stream of stored blocks, each at most 65535 bytes, the last one marked.
    let mut packed = vec![0x78, 0x01];
    let blocks = raw.chunks(65535).count();
    for (i, block) in raw.chunks(65535).enumerate() {
        packed.push((i + 1 == blocks) as u8);
        packed.extend((block.len() as u16).to_le_bytes());
        packed.extend((!(block.len() as u16)).to_le_bytes());
        packed.extend(block);
    }
    packed.extend(adler32(&raw).to_be_bytes());

    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut head = Vec::new();
    head.extend(width.to_be_bytes());
    head.extend(height.to_be_bytes());
    head.extend([8, 6, 0, 0, 0]); // 8 bits a channel, with alpha, nothing interlaced
    chunk(&mut out, b"IHDR", &head);
    chunk(&mut out, b"IDAT", &packed);
    chunk(&mut out, b"IEND", &[]);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_checks_are_those_of_the_format() {
        assert_eq!(crc32(b"IEND"), 0xAE42_6082);
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        // Two pixels: the signature, a header of 13 bytes, the data, the end.
        let file = png(2, 1, &[255, 0, 0, 255, 0, 0, 255, 255]);
        assert_eq!(&file[..8], b"\x89PNG\r\n\x1a\n");
        assert_eq!(&file[12..16], b"IHDR");
        assert_eq!(&file[file.len() - 8..file.len() - 4], b"IEND");
    }
}
