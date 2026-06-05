use std::collections::HashMap;
use std::io;

pub struct Register {
    registers: HashMap<char, String>,
    default_register: char,
}

impl Register {
    pub fn new() -> Self {
        let mut registers = HashMap::new();
        for c in 'a'..='z' {
            registers.insert(c, String::new());
        }
        registers.insert('"', String::new());
        registers.insert('+', String::new());

        Self {
            registers,
            default_register: '"',
        }
    }

    pub fn set(&mut self, name: char, content: &str) {
        let reg = name.to_ascii_lowercase();
        self.registers.insert(reg, content.to_string());

        if reg == '"' || reg == '+' {
            copy_to_system_clipboard(content);
        }
    }

    pub fn get(&self, name: char) -> String {
        let reg = name.to_ascii_lowercase();
        self.registers.get(&reg).cloned().unwrap_or_default()
    }

    pub fn get_default(&self) -> String {
        self.get(self.default_register)
    }
}

impl Default for Register {
    fn default() -> Self {
        Self::new()
    }
}

/// Write `text` to the host terminal's clipboard via the OSC 52 escape
/// sequence. Supported by virtually every modern terminal emulator
/// (iTerm2, Kitty, WezTerm, Alacritty, Windows Terminal, recent
/// gnome-terminal, etc.).
///
/// OSC 52 format: `ESC ] 5 2 ; c ; <base64> BEL` (or `ESC \` terminator).
/// We use the BEL terminator for maximum compatibility.
fn copy_to_system_clipboard(text: &str) {
    use std::io::Write as _;
    let mut stdout = io::stdout();
    // Empty payloads still emit the sequence; this is a no-op for terminals
    // that disallow empty clipboard writes, so we just skip them.
    if text.is_empty() {
        return;
    }
    // Base64 encode (standard alphabet with padding).
    let encoded = base64_encode(text.as_bytes());
    // Write: \x1b]52;c;<b64>\x07
    let _ = stdout.write_all(b"\x1b]52;c;");
    let _ = stdout.write_all(encoded.as_bytes());
    let _ = stdout.write_all(b"\x07");
    let _ = stdout.flush();
}

/// Minimal RFC 4648 §4 base64 encoder.
///
/// Avoids pulling in the `base64` crate for this single use. Output is
/// always padded with `=`.
fn base64_encode(input: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    let mut i = 0;
    while i + 3 <= input.len() {
        let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8) | (input[i + 2] as u32);
        out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
        out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
        out.push(ALPHABET[(n & 0x3F) as usize] as char);
        i += 3;
    }
    match input.len() - i {
        0 => {}
        1 => {
            let n = (input[i] as u32) << 16;
            out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
            out.push('=');
            out.push('=');
        }
        2 => {
            let n = ((input[i] as u32) << 16) | ((input[i + 1] as u32) << 8);
            out.push(ALPHABET[((n >> 18) & 0x3F) as usize] as char);
            out.push(ALPHABET[((n >> 12) & 0x3F) as usize] as char);
            out.push(ALPHABET[((n >> 6) & 0x3F) as usize] as char);
            out.push('=');
        }
        _ => unreachable!(),
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc4648_test_vectors() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn register_does_not_panic_for_special_chars() {
        let mut r = Register::new();
        // We can't assert clipboard output (no TTY in tests), but we
        // verify the code path runs without panicking.
        r.set('"', "hello\nworld");
        r.set('+', "tab\there");
        r.set('a', "internal");
        assert_eq!(r.get('a'), "internal");
    }

    #[test]
    fn empty_clipboard_set_is_noop() {
        let mut r = Register::new();
        // Empty set should not crash, even if the OSC 52 path is skipped.
        r.set('"', "");
        r.set('+', "");
        assert_eq!(r.get('"'), "");
    }
}
