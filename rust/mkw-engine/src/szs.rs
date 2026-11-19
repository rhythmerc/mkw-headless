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

/// `data` as a Yaz0 stream, uncompressed (every byte a literal: what the game reads, a little larger).
pub fn yaz0_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(16 + data.len() + data.len() / 8 + 1);
    out.extend_from_slice(b"Yaz0");
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(&[0; 8]);
    for chunk in data.chunks(8) {
        out.push(0xFF);
        out.extend_from_slice(chunk);
    }
    out
}

/// A U8 archive of `files` (paths as [`u8_files`] reads them back), laid out as the game's own: a nameless root, a
/// `.` folder holding everything, each folder's files then its folders, the data 32-byte aligned.
pub fn u8_archive(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    // (the tree: each folder's files and subfolders)
    #[derive(Default)]
    struct Folder<'a> {
        files: BTreeMap<&'a str, &'a [u8]>,
        folders: BTreeMap<&'a str, Folder<'a>>,
    }
    let mut root = Folder::default();
    for (path, data) in files {
        let mut parts: Vec<&str> = path.split('/').collect();
        let name = parts.pop().unwrap();
        let mut folder = &mut root;
        for part in parts {
            folder = folder.folders.entry(part).or_default();
        }
        folder.files.insert(name, data);
    }
    // Nodes: (is a folder, name, parent or data index, past-the-end or size).
    struct Node<'a> {
        folder: bool,
        name: &'a str,
        a: usize,
        b: usize,
        data: Option<&'a [u8]>,
    }
    fn walk<'a>(folder: &Folder<'a>, name: &'a str, parent: usize, nodes: &mut Vec<Node<'a>>) {
        let me = nodes.len();
        nodes.push(Node { folder: true, name, a: parent, b: 0, data: None });
        for (name, data) in &folder.files {
            nodes.push(Node { folder: false, name, a: 0, b: data.len(), data: Some(data) });
        }
        for (name, sub) in &folder.folders {
            walk(sub, name, me, nodes);
        }
        nodes[me].b = nodes.len();
    }
    let mut nodes = Vec::new();
    nodes.push(Node { folder: true, name: "", a: 0, b: 0, data: None });
    walk(&root, ".", 0, &mut nodes);
    nodes[0].b = nodes.len();
    let mut names = Vec::new();
    let mut name_at = Vec::new();
    for n in &nodes {
        name_at.push(names.len());
        names.extend_from_slice(n.name.as_bytes());
        names.push(0);
    }
    let header = nodes.len() * 12 + names.len();
    let align = |n: usize| n.div_ceil(32) * 32;
    let mut at = align(0x20 + header);
    let data_start = at;
    let mut offsets = Vec::new();
    for n in &nodes {
        offsets.push(at);
        if let Some(d) = n.data {
            at = align(at + d.len());
        }
    }
    let mut out = vec![0u8; at];
    out[..4].copy_from_slice(&[0x55, 0xAA, 0x38, 0x2D]);
    out[4..8].copy_from_slice(&0x20u32.to_be_bytes());
    out[8..12].copy_from_slice(&(header as u32).to_be_bytes());
    out[12..16].copy_from_slice(&(data_start as u32).to_be_bytes());
    for (k, n) in nodes.iter().enumerate() {
        let at = 0x20 + k * 12;
        let word0 = (u32::from(n.folder) << 24) | name_at[k] as u32;
        let a = if n.folder { n.a } else { offsets[k] };
        out[at..at + 4].copy_from_slice(&word0.to_be_bytes());
        out[at + 4..at + 8].copy_from_slice(&(a as u32).to_be_bytes());
        out[at + 8..at + 12].copy_from_slice(&(n.b as u32).to_be_bytes());
        if let Some(d) = n.data {
            out[offsets[k]..offsets[k] + d.len()].copy_from_slice(d);
        }
    }
    let names_at = 0x20 + nodes.len() * 12;
    out[names_at..names_at + names.len()].copy_from_slice(&names);
    out
}

/// `files` as an `.szs` archive.
pub fn write(files: &BTreeMap<String, Vec<u8>>) -> Vec<u8> {
    yaz0_store(&u8_archive(files))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Written and read back: the same files, nested folders too.
    #[test]
    fn an_archive_reads_back_as_written() {
        let mut files = BTreeMap::new();
        files.insert("course.kmp".to_string(), vec![1, 2, 3]);
        files.insert("course.kcl".to_string(), (0..100u8).collect());
        files.insert("posteffect/posteffect.blight".to_string(), vec![9; 40]);
        assert_eq!(read(&write(&files)).unwrap(), files);
    }
}
