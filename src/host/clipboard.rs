use base64::{Engine as _, engine::general_purpose::STANDARD};

pub fn osc52(text: &str) -> Vec<u8> {
    format!("\x1b]52;c;{}\x07", STANDARD.encode(text.as_bytes())).into_bytes()
}

#[cfg(test)]
mod tests;
