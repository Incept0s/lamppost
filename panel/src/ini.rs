//! A very small INI reader and writer.
//!
//! The format is the one Python's configparser wrote for earlier versions of
//! the control panel ("key = value" under "[Section]", keys lower case), so an
//! existing lamppost-control.ini keeps working.
use std::collections::BTreeMap;
use std::path::Path;

#[derive(Default)]
pub struct Ini {
    sections: BTreeMap<String, BTreeMap<String, String>>,
}

impl Ini {
    pub fn load(path: &Path) -> Ini {
        let mut ini = Ini::default();
        let Ok(text) = std::fs::read_to_string(path) else { return ini };
        let mut section = String::from("Common");
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_string();
            } else if let Some((key, value)) = line.split_once('=') {
                ini.set(&section, key.trim(), value.trim());
            }
        }
        ini
    }

    pub fn get(&self, section: &str, key: &str) -> Option<&str> {
        self.sections.get(section)?.get(&key.to_lowercase()).map(String::as_str)
    }

    pub fn get_bool(&self, section: &str, key: &str, default: bool) -> bool {
        match self.get(section, key).map(str::to_lowercase).as_deref() {
            Some("true" | "yes" | "1" | "on") => true,
            Some("false" | "no" | "0" | "off") => false,
            _ => default,
        }
    }

    pub fn set(&mut self, section: &str, key: &str, value: impl Into<String>) {
        self.sections
            .entry(section.to_string())
            .or_default()
            .insert(key.to_lowercase(), value.into());
    }

    pub fn set_bool(&mut self, section: &str, key: &str, value: bool) {
        // written the way Python wrote it, so older versions can still read it
        self.set(section, key, if value { "True" } else { "False" });
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let mut out = String::new();
        // "Common" first, like the file the Python panel produced
        let order = ["Common", "Autostart"];
        let names: Vec<&String> = order
            .iter()
            .filter_map(|wanted| self.sections.keys().find(|s| s.as_str() == *wanted))
            .chain(self.sections.keys().filter(|s| !order.contains(&s.as_str())))
            .collect();
        for name in names {
            out.push_str(&format!("[{name}]\n"));
            for (key, value) in &self.sections[name] {
                out.push_str(&format!("{key} = {value}\n"));
            }
            out.push('\n');
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, out)
    }
}
