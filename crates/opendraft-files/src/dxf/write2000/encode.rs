//! Turning the writer's text into the bytes DXF 2000 files hold.

/// The text as ASCII bytes. DXF 2000 and later keep non-ASCII characters in
/// text as `\U+XXXX`, which reads the same whatever code page or encoding the
/// reader assumes (2007 and later files are UTF-8, 2000's are ANSI).
pub(super) fn ascii(text: &str) -> Vec<u8> {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut buffer = [0u16; 2];
            for unit in c.encode_utf16(&mut buffer) {
                out.push_str(&format!("\\U+{unit:04X}"));
            }
        }
    }
    out.into_bytes()
}
