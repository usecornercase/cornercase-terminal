use std::io::{self, Read, Write};

use ratatui::style::{Color, Modifier, Style};

use super::syntax::{self, Segments};

const TAG_NONE: u8 = 0;
const TAG_LINES: u8 = 1;
pub(super) const LINE_CAP: usize = 1_000_000;
pub(super) const SEGMENT_CAP: usize = 100_000;

pub(super) fn serve() -> io::Result<()> {
    let stdin = io::stdin();
    let mut input = stdin.lock();
    let stdout = io::stdout();
    let mut output = io::BufWriter::new(stdout.lock());
    loop {
        let language = match read_string(&mut input) {
            Ok(language) if !language.is_empty() => language,
            Ok(_) => return Ok(()),
            Err(err) if err.kind() == io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(err) => return Err(err),
        };
        let source = read_string(&mut input)?;
        write_response(&mut output, syntax::parse(&source, &language).as_deref())?;
        output.flush()?;
    }
}

pub(super) fn write_response(output: &mut impl Write, lines: Option<&[Segments]>) -> io::Result<()> {
    let Some(lines) = lines else {
        return output.write_all(&[TAG_NONE]);
    };
    output.write_all(&[TAG_LINES])?;
    write_u32(output, lines.len())?;
    for line in lines {
        write_line(output, line)?;
    }
    Ok(())
}

fn write_line(output: &mut impl Write, line: &Segments) -> io::Result<()> {
    write_u32(output, line.len())?;
    for (text, style) in line {
        output.write_all(&pack(*style).to_le_bytes())?;
        write_u32(output, text.len())?;
        output.write_all(text.as_bytes())?;
    }
    Ok(())
}

fn write_u32(output: &mut impl Write, len: usize) -> io::Result<()> {
    let len = u32::try_from(len).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    output.write_all(&len.to_le_bytes())
}

pub(super) fn write_string(output: &mut impl Write, text: &str) -> io::Result<()> {
    write_u32(output, text.len())?;
    output.write_all(text.as_bytes())
}

pub(super) fn read_string(input: &mut impl Read) -> io::Result<String> {
    let len = read_u32(input)?;
    if len > syntax::MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "highlight request is too large"));
    }
    let mut buf = vec![0; len];
    input.read_exact(&mut buf)?;
    String::from_utf8(buf).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

pub(super) fn read_u32(input: &mut impl Read) -> io::Result<usize> {
    let mut buf = [0; 4];
    input.read_exact(&mut buf)?;
    Ok(u32::from_le_bytes(buf) as usize)
}

pub(super) fn pack(style: Style) -> u16 {
    let mut bits: u16 = match style.fg {
        Some(Color::Magenta) => 1,
        Some(Color::Blue) => 2,
        Some(Color::Green) => 3,
        Some(Color::DarkGray) => 4,
        Some(Color::Cyan) => 5,
        Some(Color::Yellow) => 6,
        Some(Color::Red) => 7,
        _ => 0,
    };
    let mods = style.add_modifier;
    if mods.contains(Modifier::ITALIC) {
        bits |= 1 << 4;
    }
    if mods.contains(Modifier::BOLD) {
        bits |= 1 << 5;
    }
    if mods.contains(Modifier::UNDERLINED) {
        bits |= 1 << 6;
    }
    if mods.contains(Modifier::CROSSED_OUT) {
        bits |= 1 << 7;
    }
    bits
}

pub(super) fn unpack(bits: u16) -> Style {
    let mut style = match bits & 0xf {
        1 => Style::new().fg(Color::Magenta),
        2 => Style::new().fg(Color::Blue),
        3 => Style::new().fg(Color::Green),
        4 => Style::new().fg(Color::DarkGray),
        5 => Style::new().fg(Color::Cyan),
        6 => Style::new().fg(Color::Yellow),
        7 => Style::new().fg(Color::Red),
        _ => Style::new(),
    };
    let bits = u8::try_from(bits).unwrap_or(u8::MAX);
    if bits & (1 << 4) != 0 {
        style = style.add_modifier(Modifier::ITALIC);
    }
    if bits & (1 << 5) != 0 {
        style = style.add_modifier(Modifier::BOLD);
    }
    if bits & (1 << 6) != 0 {
        style = style.add_modifier(Modifier::UNDERLINED);
    }
    if bits & (1 << 7) != 0 {
        style = style.add_modifier(Modifier::CROSSED_OUT);
    }
    style
}
