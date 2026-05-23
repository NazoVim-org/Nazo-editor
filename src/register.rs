use arboard::Clipboard;
use std::collections::HashMap;
use std::sync::Mutex;

pub struct Register {
    registers: HashMap<char, String>,
    default_register: char,
    system_clipboard: Mutex<Option<Clipboard>>,
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
            system_clipboard: Mutex::new(Clipboard::new().ok()),
        }
    }

    pub fn set(&mut self, name: char, content: &str) {
        let reg = name.to_ascii_lowercase();
        self.registers.insert(reg, content.to_string());

        if reg == '"' || reg == '+' {
            self.set_system_clipboard(content);
        }
    }

    pub fn get(&self, name: char) -> String {
        let reg = name.to_ascii_lowercase();
        self.registers.get(&reg).cloned().unwrap_or_default()
    }

    pub fn get_default(&self) -> String {
        self.get(self.default_register)
    }

    fn set_system_clipboard(&self, content: &str) {
        if let Ok(mut clipboard) = self.system_clipboard.lock() {
            if let Some(ref mut cb) = *clipboard {
                let _ = cb.set_text(content);
            }
        }
    }
}

impl Default for Register {
    fn default() -> Self {
        Self::new()
    }
}
