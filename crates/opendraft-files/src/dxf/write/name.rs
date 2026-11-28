//! Names as DXF wants them: forbidden characters replaced, Latin-1 letters
//! kept (the file is written in code page 1252) and anything beyond written
//! as `\U+XXXX`.

/// Characters DXF forbids in symbol names.
const FORBIDDEN: &[char] = &[
    '<', '>', '/', '\\', '"', ':', ';', '?', '*', '|', '=', '`', ',',
];

/// A block, style or linetype name: forbidden characters replaced.
pub(crate) fn dxf_name(name: &str) -> String {
    name_with(name, false)
}

/// A layer name: the same, except `|`, which an external reference's bound
/// layer names (`<xref>|<layer>`) use and AutoCAD keeps.
pub(crate) fn dxf_layer_name(name: &str) -> String {
    name_with(name, true)
}

fn name_with(name: &str, keep_pipe: bool) -> String {
    let mut out = String::new();
    for c in name.chars() {
        if (FORBIDDEN.contains(&c) && !(keep_pipe && c == '|')) || c.is_control() {
            out.push('_');
        } else if u32::from(c) < 0x100 {
            out.push(c);
        } else {
            let mut buffer = [0u16; 2];
            for unit in c.encode_utf16(&mut buffer) {
                out.push_str(&format!("\\U+{unit:04X}"));
            }
        }
    }
    out
}
