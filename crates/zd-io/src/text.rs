//! Text helpers shared by the CSV readers and writers.

/// Excel saves CSV as UTF-8 (with a byte-order mark) or in the Windows code page; the latter is
/// read as Latin-1, which agrees with it for every letter in file names.
pub(crate) fn decode_text(bytes: &[u8]) -> String {
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    match std::str::from_utf8(bytes) {
        Ok(text) => text.to_string(),
        Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
    }
}

/// Splits one CSV line: commas separate fields, a field may be wrapped in double quotes, and `""`
/// inside quotes is one quote. Unquoted fields are trimmed.
pub(crate) fn csv_fields(line: &str) -> Result<Vec<String>, &'static str> {
    let mut fields = Vec::new();
    let mut chars = line.chars().peekable();
    loop {
        while chars.next_if(|c| *c == ' ' || *c == '\t').is_some() {}
        let mut field = String::new();
        if chars.next_if_eq(&'"').is_some() {
            loop {
                match chars.next() {
                    None => return Err("a quoted cell is not closed"),
                    Some('"') if chars.next_if_eq(&'"').is_some() => field.push('"'),
                    Some('"') => break,
                    Some(c) => field.push(c),
                }
            }
            while let Some(c) = chars.next_if(|c| *c != ',') {
                if !c.is_whitespace() {
                    return Err("text after the closing quote of a cell");
                }
            }
        } else {
            while let Some(c) = chars.next_if(|c| *c != ',') {
                field.push(c);
            }
            field.truncate(field.trim_end().len());
        }
        fields.push(field);
        if chars.next().is_none() {
            return Ok(fields);
        }
    }
}

/// One CSV cell, quoted when it holds a comma, a quote, a line break or edge spaces.
pub(crate) fn csv_cell(text: &str) -> String {
    if text.contains([',', '"', '\n']) || text.trim() != text {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}
