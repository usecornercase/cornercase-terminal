#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Value<'a> {
    Varint(u64),
    Bytes(&'a [u8]),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Field<'a> {
    pub(super) number: u32,
    pub(super) value: Value<'a>,
}

fn varint(data: &[u8], at: &mut usize) -> Option<u64> {
    let mut out = 0_u64;
    for shift in (0..64).step_by(7) {
        let byte = *data.get(*at)?;
        *at += 1;
        out |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(out);
        }
    }
    None
}

fn skip(data: &[u8], at: &mut usize, len: usize) -> Option<()> {
    let end = at.checked_add(len).filter(|end| *end <= data.len())?;
    *at = end;
    Some(())
}

pub(super) fn fields(data: &[u8]) -> Option<Vec<Field<'_>>> {
    let mut out = Vec::new();
    let mut at = 0;
    while at < data.len() {
        let tag = varint(data, &mut at)?;
        let number = u32::try_from(tag >> 3).ok().filter(|number| *number > 0)?;
        match tag & 7 {
            0 => out.push(Field { number, value: Value::Varint(varint(data, &mut at)?) }),
            1 => skip(data, &mut at, 8)?,
            2 => {
                let len = usize::try_from(varint(data, &mut at)?).ok()?;
                let start = at;
                skip(data, &mut at, len)?;
                out.push(Field { number, value: Value::Bytes(&data[start..at]) });
            }
            5 => skip(data, &mut at, 4)?,
            _ => return None,
        }
    }
    Some(out)
}

pub(super) fn bytes<'a>(fields: &[Field<'a>], number: u32) -> Option<&'a [u8]> {
    fields.iter().find_map(|f| match f.value {
        Value::Bytes(bytes) if f.number == number => Some(bytes),
        _ => None,
    })
}

pub(super) fn all_bytes<'a>(fields: &[Field<'a>], number: u32) -> Vec<&'a [u8]> {
    fields
        .iter()
        .filter_map(|f| match f.value {
            Value::Bytes(bytes) if f.number == number => Some(bytes),
            _ => None,
        })
        .collect()
}

pub(super) fn number(fields: &[Field<'_>], number: u32) -> Option<u64> {
    fields.iter().find_map(|f| match f.value {
        Value::Varint(value) if f.number == number => Some(value),
        _ => None,
    })
}

pub(super) fn text(fields: &[Field<'_>], number: u32) -> Option<String> {
    String::from_utf8(bytes(fields, number)?.to_vec()).ok()
}

pub(super) fn blob_id(bytes: &[u8]) -> Option<String> {
    use std::fmt::Write;
    if bytes.len() == 32 {
        return Some(bytes.iter().fold(String::with_capacity(64), |mut hex, b| {
            let _ = write!(hex, "{b:02x}");
            hex
        }));
    }
    let text = std::str::from_utf8(bytes).ok()?;
    (text.len() == 64 && text.bytes().all(|b| b.is_ascii_hexdigit())).then(|| text.to_ascii_lowercase())
}

pub(super) fn from_hex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) || !text.is_ascii() {
        return None;
    }
    (0..text.len()).step_by(2).map(|at| u8::from_str_radix(&text[at..at + 2], 16).ok()).collect()
}

#[cfg(test)]
mod tests;
