//! The game's archives: Yaz0-compressed U8 (`.szs`), read into their files by path.
use std::collections::BTreeMap;

/// A Yaz0 stream's bytes, decompressed.
pub fn yaz0(data: &[u8]) -> Result<Vec<u8>, String> {
    if data.len() < 16 || &data[..4] != b"Yaz0" {
        return Err("not Yaz0".into());
    }
    let size = u32::from_be_bytes(data[4..8].try_into().unwrap()) as usize;
    let mut out = Vec::with_capacity(size);
    let mut i = 16;
    let byte = |i: usize| data.get(i).copied().ok_or("Yaz0 truncated");
    while out.len() < size {
        let code = byte(i)?;
        i += 1;
        for bit in 0..8 {
            if out.len() >= size {
                break;
            }
            if code & (0x80 >> bit) != 0 {
                out.push(byte(i)?);
                i += 1;
            } else {
                let (b1, b2) = (byte(i)? as usize, byte(i + 1)? as usize);
                i += 2;
                let distance = ((b1 & 0xF) << 8 | b2) + 1;
                let count = match b1 >> 4 {
                    0 => {
                        i += 1;
                        byte(i - 1)? as usize + 0x12
                    }
                    n => n + 2,
                };
                if distance > out.len() {
                    return Err("Yaz0 back-reference before the start".into());
                }
                for _ in 0..count {
                    out.push(out[out.len() - distance]);
                }
            }
        }
    }
    Ok(out)
}

/// A U8 archive's files by path (`a/b.brres`; a leading `./` dropped).
pub fn u8_files(data: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let be32 = |at: usize| data.get(at..at + 4).map(|b| u32::from_be_bytes(b.try_into().unwrap()) as usize).ok_or("U8 truncated");
    if data.get(..4) != Some(&[0x55, 0xAA, 0x38, 0x2D]) {
        return Err("not U8".into());
    }
    let root = be32(4)?;
    let count = be32(root + 8)?;
    let names = root + count * 12;
    let mut files = BTreeMap::new();
    // (each folder: its name and the index past its last node)
    let mut folders: Vec<(String, usize)> = Vec::new();
    for k in 0..count {
        let node = root + k * 12;
        let kind = *data.get(node).ok_or("U8 truncated")?;
        let name_at = be32(node)? & 0x00FF_FFFF;
        let (offset, size) = (be32(node + 4)?, be32(node + 8)?);
        let start = names + name_at;
        let end = data[start..].iter().position(|&b| b == 0).map(|e| start + e).ok_or("U8 name")?;
        let name = String::from_utf8_lossy(&data[start..end]).into_owned();
        while folders.last().is_some_and(|(_, past)| *past <= k) {
            folders.pop();
        }
        let path: Vec<&str> = folders.iter().map(|(n, _)| n.as_str()).filter(|n| !n.is_empty() && *n != ".").collect();
        if kind == 1 {
            folders.push((name, size));
        } else {
            let full = if path.is_empty() { name } else { format!("{}/{name}", path.join("/")) };
            files.insert(full, data.get(offset..offset + size).ok_or("U8 file past the end")?.to_vec());
        }
    }
    Ok(files)
}

/// An `.szs` archive's files.
pub fn read(data: &[u8]) -> Result<BTreeMap<String, Vec<u8>>, String> {
    u8_files(&yaz0(data)?)
}
