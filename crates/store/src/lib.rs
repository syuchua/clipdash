use std::{fs, io::{self, BufRead, BufReader, Write}, path::PathBuf};
use clipdash_core::{Item, ItemKind};

#[derive(Default)]
pub struct Store {
    items: Vec<Item>,
}

impl Store {
    pub fn new_in_memory() -> Self { Store { items: Vec::new() } }
    pub fn put(&mut self, item: Item) { self.items.push(item) }
    pub fn all(&self) -> &[Item] { &self.items }
}

pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self { Self { path: path.into() } }

    fn encode_item(it: &Item) -> String {
        let kind = match it.kind { ItemKind::Text => 'T', ItemKind::Image => 'I', ItemKind::Html => 'H' };
        let mut hex = String::with_capacity(it.data.len() * 2);
        for b in &it.data { hex.push(nibble_to_hex(b >> 4)); hex.push(nibble_to_hex(b & 0x0F)); }
        let mime = it.mime.clone().unwrap_or_default().replace('|', ":");
        let path = it.file_path.clone().unwrap_or_default().replace('|', ":");
        format!("{}|{}|{}|{}|{}|{}|{}|{}", it.id, kind, if it.pinned {1}else{0}, it.ts_ms, mime, path, it.data.len(), hex)
    }

    fn decode_item(line: &str) -> Option<Item> {
        let mut parts = line.split('|');
        let id: u64 = parts.next()?.parse().ok()?;
        let kch = parts.next()?.chars().next()?;
        let kind = match kch { 'T' => ItemKind::Text, 'I' => ItemKind::Image, 'H' => ItemKind::Html, _ => return None };
        let pinned = match parts.next()? { "1" => true, _ => false };
        // v4: ts_ms | mime | path | len | hex
        let n1 = parts.next()?;
        let (ts_ms, mime_opt, path_opt, len_s, hex_s) = if let Ok(ts) = n1.parse::<i64>() {
            let mime = parts.next().unwrap_or("");
            let maybe_path_or_len = parts.next().unwrap_or("");
            // if parse usize ok -> v3 (no path), else v4 with path
            if let Ok(_ok_len) = maybe_path_or_len.parse::<usize>() {
                let len_s = maybe_path_or_len;
                let hex_s = parts.next()?;
                (ts, if mime.is_empty() { None } else { Some(mime.to_string()) }, None, len_s, hex_s)
            } else {
                let path = maybe_path_or_len;
                let len_s = parts.next()?;
                let hex_s = parts.next()?;
                (ts, if mime.is_empty() { None } else { Some(mime.to_string()) }, if path.is_empty() { None } else { Some(path.to_string()) }, len_s, hex_s)
            }
        } else {
            // v1/v2 fallback
            // If header says v2, we saw ts in the stream earlier; but at this point we don't know header. We'll detect by count: if remaining parts are 1 -> hex (v1), else 2 -> hex (v2)
            let len_s = n1; // actually len
            let hex_s = parts.next()?;
            (current_time_ms(), None, None, len_s, hex_s)
        };
        let len: usize = len_s.parse().ok()?;
        let data = hex_to_bytes(hex_s)?;
        if data.len() != len { return None; }
        Some(Item { id, kind, data, pinned, ts_ms, mime: mime_opt, file_path: path_opt })
    }

    pub fn save(&self, items: &[Item]) -> io::Result<()> {
        let dir = self.path.parent().map(|p| p.to_path_buf()).unwrap_or_else(|| PathBuf::from("."));
        fs::create_dir_all(&dir)?;
        let tmp = self.path.with_extension("tmp");
        let mut f = fs::File::create(&tmp)?;
        f.write_all(b"CLIPDASHv4\n")?;
        for it in items { writeln!(f, "{}", Self::encode_item(it))?; }
        f.flush()?;
        fs::rename(tmp, &self.path)?;
        Ok(())
    }

    pub fn load(&self) -> io::Result<Vec<Item>> {
        let f = match fs::File::open(&self.path) { Ok(f) => f, Err(e) if e.kind()==io::ErrorKind::NotFound => return Ok(Vec::new()), Err(e) => return Err(e) };
        let mut rdr = BufReader::new(f);
        let mut first = String::new();
        rdr.read_line(&mut first)?;
        if !(first.starts_with("CLIPDASHv4") || first.starts_with("CLIPDASHv3") || first.starts_with("CLIPDASHv2") || first.starts_with("CLIPDASHv1")) { return Ok(Vec::new()); }
        let mut items = Vec::new();
        for line in rdr.lines() {
            let line = line?;
            if line.trim().is_empty() { continue; }
            if let Some(it) = Self::decode_item(&line) { items.push(it); }
        }
        Ok(items)
    }
}

fn nibble_to_hex(n: u8) -> char { match n & 0x0F { 0..=9 => (b'0'+n) as char, 10..=15 => (b'a'+(n-10)) as char, _ => unreachable!() } }

fn hex_to_bytes(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 { return None; }
    let mut out = Vec::with_capacity(s.len()/2);
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let hi = hex_val(bytes[i])?; let lo = hex_val(bytes[i+1])?;
        out.push((hi<<4) | lo);
        i += 2;
    }
    Some(out)
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(10 + b - b'a'),
        b'A'..=b'F' => Some(10 + b - b'A'),
        _ => None,
    }
}

fn current_time_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let dur = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    (dur.as_secs() as i64)*1000 + (dur.subsec_millis() as i64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_store() {
        let _s = Store::new_in_memory();
    }

    #[test]
    fn encode_decode_roundtrip() {
        let it = Item { id: 42, kind: ItemKind::Text, data: b"hello".to_vec(), pinned: true, ts_ms: 123456, mime: Some("text/plain".into()) };
        let line = FileStore::encode_item(&it);
        let dec = FileStore::decode_item(&line).unwrap();
        assert_eq!(dec.id, 42);
        assert!(dec.pinned);
        assert_eq!(String::from_utf8(dec.data).unwrap(), "hello");
        assert_eq!(dec.ts_ms, 123456);
        assert_eq!(dec.mime.as_deref(), Some("text/plain"));
    }
}
