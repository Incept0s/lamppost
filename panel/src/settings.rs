//! lamppost-control.ini - the same keys the Python control panel used.
use crate::ini::Ini;
use std::path::PathBuf;

pub fn prefix() -> PathBuf {
    std::env::var_os("LAMPPOST_PREFIX")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/opt/lamppost"))
}

pub fn path(parts: &str) -> PathBuf {
    prefix().join(parts)
}

pub const MODULE_KEYS: [&str; 5] = ["apache", "mariadb", "ftp", "mail", "tomcat"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Look {
    Modern,
    Classic,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    System,
    Light,
    Dark,
}

pub struct Settings {
    ini: Ini,
    file: PathBuf,
}

impl Settings {
    pub fn load() -> Settings {
        let file = path("lamppost-control.ini");
        Settings { ini: Ini::load(&file), file }
    }

    pub fn save(&self) {
        if let Err(e) = self.ini.save(&self.file) {
            eprintln!("lamppost: could not write {}: {e}", self.file.display());
        }
    }

    pub fn editor(&self) -> String {
        self.ini
            .get("Common", "Editor")
            .map(str::to_string)
            .unwrap_or_else(|| {
                if which("gnome-text-editor").is_some() { "gnome-text-editor".into() } else { "xdg-open".into() }
            })
    }

    pub fn set_editor(&mut self, value: &str) {
        let value = if value.trim().is_empty() { "xdg-open" } else { value.trim() };
        self.ini.set("Common", "Editor", value);
    }

    pub fn look(&self) -> Look {
        match self.ini.get("Common", "Appearance") {
            Some("classic") => Look::Classic,
            _ => Look::Modern,
        }
    }

    pub fn set_look(&mut self, look: Look) {
        self.ini.set("Common", "Appearance", match look {
            Look::Classic => "classic",
            Look::Modern => "modern",
        });
    }

    pub fn scheme(&self) -> Scheme {
        match self.ini.get("Common", "ColorScheme") {
            Some("light") => Scheme::Light,
            Some("dark") => Scheme::Dark,
            _ => Scheme::System,
        }
    }

    pub fn set_scheme(&mut self, scheme: Scheme) {
        self.ini.set("Common", "ColorScheme", match scheme {
            Scheme::Light => "light",
            Scheme::Dark => "dark",
            Scheme::System => "system",
        });
    }

    pub fn check_ports(&self) -> bool {
        self.ini.get_bool("Common", "CheckDefaultPorts", true)
    }

    pub fn set_check_ports(&mut self, value: bool) {
        self.ini.set_bool("Common", "CheckDefaultPorts", value);
    }

    pub fn check_updates(&self) -> bool {
        self.ini.get_bool("Common", "CheckUpdatesAtStart", true)
    }

    pub fn set_check_updates(&mut self, value: bool) {
        self.ini.set_bool("Common", "CheckUpdatesAtStart", value);
    }

    pub fn minimize_to_tray(&self) -> bool {
        self.ini.get_bool("Common", "MinimizeToTray", true)
    }

    pub fn set_minimize_to_tray(&mut self, value: bool) {
        self.ini.set_bool("Common", "MinimizeToTray", value);
    }

    pub fn autostart(&self, key: &str) -> bool {
        self.ini.get_bool("Autostart", key, false)
    }

    pub fn set_autostart(&mut self, key: &str, value: bool) {
        self.ini.set_bool("Autostart", key, value);
    }

    /// Window size of the last session, if it still fits on a screen.
    pub fn window_size(&self) -> Option<[f32; 2]> {
        let raw = self.ini.get("Common", "WindowSize")?;
        let mut parts = raw.split(',').filter_map(|v| v.trim().parse::<f32>().ok());
        let (w, h) = (parts.next()?, parts.next()?);
        (w >= 700.0 && h >= 480.0 && w < 10000.0 && h < 10000.0).then_some([w, h])
    }

    pub fn set_window_size(&mut self, size: [f32; 2]) {
        self.ini.set("Common", "WindowSize", format!("{:.0},{:.0}", size[0], size[1]));
    }
}

/// ~/.config/autostart entry: start the panel (tray only) at login.
pub fn login_autostart_file() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    base.join("autostart/lamppost-control.desktop")
}

pub fn login_autostart() -> bool {
    login_autostart_file().exists()
}

pub fn set_login_autostart(enabled: bool) -> std::io::Result<()> {
    let file = login_autostart_file();
    if enabled {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let exe = own_program();
        std::fs::write(
            &file,
            format!(
                "[Desktop Entry]\nType=Application\nName=LAMPPost Control Panel\n\
                 Comment=LAMPPost tray icon\nExec={} --tray\nIcon=lamppost\n\
                 Terminal=false\nX-GNOME-Autostart-enabled=true\n",
                exe.display()
            ),
        )
    } else if file.exists() {
        std::fs::remove_file(file)
    } else {
        Ok(())
    }
}

/// The program to start again later (the window, the tray, at login).
///
/// Inside an AppImage, current_exe() points into a temporary mount that is
/// gone once the AppImage exits - the AppImage file itself is what to run.
pub fn own_program() -> PathBuf {
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|p| p.is_file())
        .or_else(|| std::env::current_exe().ok())
        .unwrap_or_else(|| path("lamppost-control"))
}

pub fn which(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| candidate.is_file())
}
