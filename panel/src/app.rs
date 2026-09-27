//! The control panel window: module list, log, dialogs and the tray.
use crate::modules::{self, Admin, Module};
use crate::settings::{self, Look, Scheme, Settings};
use crate::theme::{self, Palette};
use crate::widgets::{
    card, caption, cell, chip, dot, filled_button, heading, logo, muted, plain_button, section, segmented,
    setting_row, square_ticks, tag,
};
use crate::{cli, packages, sysinfo};
use crossbeam_channel::{Receiver, Sender};
use eframe::egui::{self, Align, FontId, Layout, RichText, Vec2};
use std::time::{Duration, Instant};

pub enum Msg {
    /// a second start, or the tray: bring this window forward
    ShowWindow,
    /// the title bar's close button: to the tray, or really quit
    CloseWindow,
    Quit,
    /// modules that could not start because their packages are missing
    OfferInstall(Vec<String>),
    Cli(cli::Done),
    Checked(Result<Vec<(String, String)>, String>),
    Installed(i32, String),
    SchemeChanged,
}

struct LogLine {
    time: String,
    source: String,
    text: String,
    error: bool,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Dialog {
    None,
    Config,
    Updates,
    Netstat,
    Services,
    About,
    Install,
}

/// The "install missing packages" window.
#[derive(PartialEq)]
enum Setup {
    /// asking the package manager what is missing
    Checking,
    Ready,
    /// pkexec and the package manager are running
    Installing,
    Cancelled,
    Failed(String),
}

pub struct App {
    settings: Settings,
    look: Look,
    scheme: Scheme,
    palette: Palette,
    modules: Vec<Module>,
    log: Vec<LogLine>,
    tx: Sender<Msg>,
    rx: Receiver<Msg>,
    dialog: Dialog,
    editor_edit: String,
    // updates
    installed: Vec<(String, String)>,
    upgrades: Vec<(String, String)>,
    selected: Vec<bool>,
    update_status: String,
    update_output: String,
    checking: bool,
    installing: bool,
    restart_after: Vec<String>,
    pending_install: Vec<String>,
    updates_available: usize,
    checked_at: Option<String>,
    // cached dialog data
    sockets: Vec<sysinfo::Socket>,
    setup_keys: Vec<String>,
    setup_packages: Vec<String>,
    setup_state: Setup,
    netstat_only_ours: bool,
    services: Vec<(String, String, String, String)>,
    // timing and window state
    next_refresh: Instant,
    fast_until: Instant,
    quitting: bool,
    size: [f32; 2],
}

const SIDEBAR: f32 = 132.0;

impl App {
    pub fn new(ctx: &egui::Context, rx: Receiver<Msg>, tx: Sender<Msg>) -> App {
        theme::install_fonts(ctx);
        let settings = Settings::load();
        let (look, scheme) = (settings.look(), settings.scheme());
        let palette = theme::palette(look, scheme);
        theme::apply(ctx, &palette, look);

        let notify = tx.clone();
        let repaint = ctx.clone();
        theme::watch_system_scheme(move || {
            let _ = notify.send(Msg::SchemeChanged);
            repaint.request_repaint();
        });

        let mut app = App {
            settings,
            look,
            scheme,
            palette,
            modules: modules::all(),
            log: Vec::new(),
            tx,
            rx,
            dialog: Dialog::None,
            editor_edit: String::new(),
            installed: Vec::new(),
            upgrades: Vec::new(),
            selected: vec![false; packages::components().len()],
            update_status: String::new(),
            update_output: String::new(),
            checking: false,
            installing: false,
            restart_after: Vec::new(),
            pending_install: Vec::new(),
            updates_available: 0,
            checked_at: None,
            sockets: Vec::new(),
            setup_keys: Vec::new(),
            setup_packages: Vec::new(),
            setup_state: Setup::Checking,
            netstat_only_ours: false,
            services: Vec::new(),
            next_refresh: Instant::now(),
            fast_until: Instant::now(),
            quitting: false,
            size: [940.0, 660.0],
        };
        app.startup_log();
        app
    }

    // ---------------------------------------------------------------- logging
    fn log(&mut self, source: &str, text: impl Into<String>) {
        self.push_log(source, text.into(), false);
    }

    fn error(&mut self, source: &str, text: impl Into<String>) {
        self.push_log(source, text.into(), true);
    }

    fn push_log(&mut self, source: &str, text: String, error: bool) {
        let line = LogLine {
            time: chrono::Local::now().format("%H:%M:%S").to_string(),
            source: source.to_string(),
            text,
            error,
        };
        eprintln!("lamppost: {} [{}] {}", line.time, line.source, line.text);
        self.log.push(line);
        if self.log.len() > 500 {
            self.log.drain(..self.log.len() - 500);
        }
    }

    fn startup_log(&mut self) {
        self.log("main", format!("LAMPPost Control Panel {} starting", env!("CARGO_PKG_VERSION")));
        self.log("main", format!("Host: {}, {}", sysinfo::os_pretty_name(), sysinfo::machine()));
        let user = std::env::var("USER").unwrap_or_else(|_| "current user".into());
        self.log("main", format!("Privileges: unprivileged, modules run as user '{user}'"));
        self.log("main", format!("Instance directory: {}", settings::prefix().display()));

        let mut missing: Vec<String> = Vec::new();
        for (name, candidates) in packages::required_programs() {
            if !packages::have_program(&candidates) {
                missing.push(format!("{name} ({})", candidates.join(" or ")));
            }
        }
        for file in [
            settings::path("lamppost"),
            settings::path("apache/conf/httpd.conf"),
            settings::path("mariadb/data/mysql"),
        ] {
            if !file.exists() {
                missing.push(file.display().to_string());
            }
        }
        if sysinfo::unprivileged_port_start() > 21 {
            missing.push("net.ipv4.ip_unprivileged_port_start > 21 (ports below 1024 locked)".into());
        }
        if missing.is_empty() {
            self.log("main", "Dependency check passed");
        } else {
            for item in missing {
                self.error("main", format!("Dependency check failed: {item} not found"));
            }
            self.error("main", "Run install.sh or lamppost-setup to install the missing components");
        }
        if let Some(group) = sysinfo::web_group().filter(|g| !sysinfo::in_group(g)) {
            self.error("main", format!("Group '{group}' not active in the current session"));
            self.error("main", "phpMyAdmin cannot read its configuration until the next login");
        }

        self.refresh_versions();
        self.refresh_modules();
        if self.settings.check_ports() {
            let busy = sysinfo::busy_ports();
            let conflicts: Vec<(String, u16, String)> = self
                .modules
                .iter()
                .filter(|m| !m.running())
                .flat_map(|m| {
                    m.ports.iter().filter_map(|port| {
                        busy.get(port).map(|owner| (m.name.to_string(), *port, owner.clone()))
                    })
                })
                .collect();
            for (name, port, owner) in conflicts {
                self.port_problem(&name, port, &owner);
            }
        }
        let autostart: Vec<String> = settings::MODULE_KEYS
            .iter()
            .filter(|key| self.settings.autostart(key))
            .filter(|key| self.modules.iter().any(|m| &m.key == *key && !m.running()))
            .map(|key| key.to_string())
            .collect();
        if !autostart.is_empty() {
            self.log("main", format!("Autostart: {}", autostart.join(", ")));
            self.start_modules("start", autostart);
        }
        self.log("main", format!("Monitoring {} modules (poll interval 1.5 s)", self.modules.len()));
        if self.settings.check_updates() {
            self.check_updates(false);
        }
    }

    fn port_problem(&mut self, name: &str, port: u16, owner: &str) {
        let who = if owner.is_empty() {
            "a process of another user (e.g. a system service or XAMPP)".to_string()
        } else {
            format!("\"{owner}\"")
        };
        self.error(name, format!("Port conflict: {port}/tcp is bound by {who}"));
        self.error(name, format!("{name} cannot start while {port}/tcp is in use"));
        self.error(name, "Stop the conflicting service or change the port in the module configuration");
    }

    // ---------------------------------------------------------------- actions
    fn start_modules(&mut self, action: &'static str, keys: Vec<String>) {
        let keys: Vec<String> = keys
            .into_iter()
            .filter(|key| self.modules.iter().any(|m| m.key == key && !m.busy))
            .collect();
        if keys.is_empty() {
            return;
        }
        for key in &keys {
            if let Some(module) = self.modules.iter_mut().find(|m| m.key == key) {
                module.busy = true;
                let (name, verb) = (module.name, if action == "start" { "Starting" } else { "Stopping" });
                self.log(name, format!("{verb} {name} ..."));
            }
        }
        self.poll_faster();
        let tx = self.tx.clone();
        cli::run(action, keys, move |done| {
            let _ = tx.send(Msg::Cli(done));
            crate::wake_ui();
        });
    }

    fn toggle(&mut self, key: &str) {
        let Some(module) = self.modules.iter().find(|m| m.key == key) else { return };
        if module.busy {
            return;
        }
        if module.running() {
            self.start_modules("stop", vec![key.to_string()]);
            return;
        }
        // do not even try when something else owns the port
        let busy = sysinfo::busy_ports();
        let blocked: Vec<(String, u16, String)> = module
            .ports
            .iter()
            .filter_map(|port| busy.get(port).map(|o| (module.name.to_string(), *port, o.clone())))
            .collect();
        if blocked.is_empty() {
            self.start_modules("start", vec![key.to_string()]);
        } else {
            for (name, port, owner) in blocked {
                self.port_problem(&name, port, &owner);
            }
        }
    }

    fn poll_faster(&mut self) {
        self.fast_until = Instant::now() + Duration::from_secs(6);
        self.next_refresh = Instant::now();
    }

    fn refresh_modules(&mut self) {
        let mut changes: Vec<(String, Option<i32>)> = Vec::new();
        for module in &mut self.modules {
            let was = module.running();
            module.refresh();
            if module.running() != was {
                changes.push((module.name.to_string(), module.pids.first().copied()));
            }
        }
        for (name, pid) in changes {
            match pid {
                Some(pid) => self.log(&name, format!("Running (PID {pid})")),
                None => self.log(&name, "Stopped"),
            }
        }
    }

    fn refresh_versions(&mut self) {
        self.installed = packages::installed();
        for (key, label) in packages::module_versions() {
            if let Some(module) = self.modules.iter_mut().find(|m| m.key == key) {
                module.version = label.clone();
            }
        }
    }

    fn open_entry(&mut self, target: &std::path::Path) {
        let Some(path) = cli::resolve_glob(target) else {
            self.error("main", format!("File not found (yet): {}", target.display()));
            return;
        };
        let editor = self.settings.editor();
        self.log("main", format!("Opening \"{}\"", path.display()));
        let ok = if path.is_dir() {
            cli::open_path(&path)
        } else {
            cli::open_in_editor(&editor, &path)
        };
        if !ok {
            self.error("main", format!("Could not open it with \"{editor}\" - change the editor in Config"));
        }
    }

    // --------------------------------------------------------------- messages
    fn handle_messages(&mut self, ctx: &egui::Context) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::ShowWindow => self.show_window(ctx),
                Msg::OfferInstall(keys) => {
                    self.show_window(ctx);
                    self.offer_install(keys);
                }
                Msg::CloseWindow => self.close(ctx),
                Msg::Quit => {
                    // Quit ends LAMPPost completely, tray icon included
                    crate::quit_tray();
                    self.quitting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Msg::Cli(done) => self.cli_finished(done),
                Msg::Checked(result) => self.checked(result),
                Msg::Installed(code, output) => self.installed_updates(code, output),
                Msg::SchemeChanged => {
                    if self.scheme == Scheme::System {
                        self.apply_theme(ctx);
                    }
                }
            }
        }
    }

    fn cli_finished(&mut self, done: cli::Done) {
        match done.action {
            "missing" => return self.missing_found(done),
            "install" => return self.install_finished(done),
            _ => {}
        }
        let name = if done.keys.len() == 1 {
            self.modules
                .iter()
                .find(|m| m.key == done.keys[0])
                .map(|m| m.name.to_string())
                .unwrap_or_else(|| "main".into())
        } else {
            "main".into()
        };
        for line in done.output.lines() {
            let line = line.trim();
            if line.is_empty() || line.ends_with("...") || line.contains("already running") {
                continue;
            }
            let lower = line.to_lowercase();
            let bad = lower.contains("fail") || lower.contains("error") || lower.contains("not installed");
            self.push_log(&name, line.to_string(), bad);
        }
        match done.code {
            0 => {}
            3 => self.error(&name, "Start aborted: required packages missing, installation offered"),
            code => self.error(&name, format!("{} failed (exit code {code})", done.action)),
        }
        for key in &done.keys {
            if let Some(module) = self.modules.iter_mut().find(|m| m.key == key) {
                module.busy = false;
            }
        }
        self.poll_faster();
        // exit code 3: a module is not installed - offer to install it
        if done.action == "start" && done.code == 3 {
            let keys: Vec<String> = done
                .keys
                .iter()
                .filter(|k| self.modules.iter().any(|m| m.key == *k && !m.running()))
                .cloned()
                .collect();
            if !keys.is_empty() {
                self.offer_install(keys);
            }
        }
        // updates waiting for their modules to stop
        if !self.pending_install.is_empty() && done.action == "stop" {
            let packages = std::mem::take(&mut self.pending_install);
            self.run_install(packages);
        }
    }

    // ------------------------------------------------------- missing packages
    fn offer_install(&mut self, keys: Vec<String>) {
        if self.setup_state == Setup::Installing {
            return; // one installation at a time
        }
        self.setup_keys = keys.clone();
        self.setup_packages.clear();
        self.setup_state = Setup::Checking;
        self.dialog = Dialog::Install;
        let tx = self.tx.clone();
        cli::run("missing", keys, move |done| {
            let _ = tx.send(Msg::Cli(done));
            crate::wake_ui();
        });
    }

    fn missing_found(&mut self, done: cli::Done) {
        self.setup_packages = done
            .output
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with("lamppost:"))
            .map(str::to_string)
            .collect();
        self.setup_state = if done.code != 0 {
            Setup::Failed(done.output.trim().to_string())
        } else {
            // an empty list means the packages are there but the module is not
            // set up in /opt/lamppost yet - "install" does that too. Never
            // retry the start on our own: that could go round in circles.
            Setup::Ready
        };
    }

    fn run_setup_install(&mut self) {
        self.setup_state = Setup::Installing;
        let names = self.setup_names();
        self.log("main", format!("Installing packages for {names}: {}", self.setup_packages.join(" ")));
        let tx = self.tx.clone();
        cli::run("install", self.setup_keys.clone(), move |done| {
            let _ = tx.send(Msg::Cli(done));
            crate::wake_ui();
        });
    }

    fn install_finished(&mut self, done: cli::Done) {
        let names = self.setup_names();
        match done.code {
            0 => {
                self.log("main", format!("Packages for {names} installed"));
                self.dialog = Dialog::None;
                self.setup_state = Setup::Checking;
                self.refresh_versions();
                let keys = std::mem::take(&mut self.setup_keys);
                self.start_modules("start", keys);
            }
            // pkexec: 126 = authentication dismissed, 127 = not authorised
            126 | 127 => {
                self.setup_state = Setup::Cancelled;
                self.error("main", "Installation cancelled: authentication was not completed");
            }
            code => {
                let tail: Vec<&str> = done.output.lines().rev().take(8).collect();
                let tail: Vec<&str> = tail.into_iter().rev().collect();
                self.setup_state = Setup::Failed(tail.join("\n"));
                self.error("main", format!("Installation for {names} failed (exit code {code})"));
            }
        }
    }

    fn setup_names(&self) -> String {
        self.setup_keys
            .iter()
            .filter_map(|k| self.modules.iter().find(|m| m.key == k).map(|m| m.name))
            .collect::<Vec<_>>()
            .join(", ")
    }

    // ---------------------------------------------------------------- updates
    fn check_updates(&mut self, refresh: bool) {
        if self.checking || self.installing {
            return;
        }
        self.checking = true;
        self.update_status = if refresh {
            "Asking the repositories for newer versions ...".into()
        } else {
            "Looking for newer versions ...".into()
        };
        let (program, args) = packages::check_command(refresh);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let output = std::process::Command::new(&program).args(&args).output();
            let result = match output {
                Ok(out) => {
                    let text = String::from_utf8_lossy(&out.stdout).to_string();
                    if !out.status.success() && text.trim().is_empty() {
                        Err(String::from_utf8_lossy(&out.stderr).trim().to_string())
                    } else {
                        Ok(packages::parse_upgrades(&text))
                    }
                }
                Err(e) => Err(format!("could not run {program}: {e}")),
            };
            let _ = tx.send(Msg::Checked(result));
        });
    }

    fn checked(&mut self, result: Result<Vec<(String, String)>, String>) {
        self.checking = false;
        match result {
            Ok(upgrades) => {
                self.checked_at = Some(chrono::Local::now().format("%H:%M").to_string());
                self.upgrades = upgrades;
                let components = packages::components();
                self.selected = components
                    .iter()
                    .map(|c| c.packages.iter().any(|p| self.upgrades.iter().any(|(name, _)| name == p)))
                    .collect();
                self.updates_available = self.selected.iter().filter(|s| **s).count();
                self.update_status = if self.updates_available == 0 {
                    "Everything is up to date.".into()
                } else {
                    format!("{} component(s) can be updated.", self.updates_available)
                };
                if self.updates_available > 0 && self.dialog != Dialog::Updates {
                    let names: Vec<&str> = self.upgrades.iter().map(|(n, _)| n.as_str()).collect();
                    self.log("updates", format!("New versions available: {}", names.join(", ")));
                    self.log("updates", "Install them with the Updates button.");
                }
            }
            Err(message) => {
                self.update_status = "Could not reach the repositories. Are you online?".into();
                if !message.is_empty() {
                    self.update_output = message;
                }
            }
        }
    }

    fn install_updates(&mut self) {
        let components = packages::components();
        let chosen: Vec<String> = components
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selected.get(*i).copied().unwrap_or(false))
            .flat_map(|(_, c)| c.packages.iter())
            .filter(|p| self.upgrades.iter().any(|(name, _)| name == *p))
            .map(|p| p.to_string())
            .collect();
        if chosen.is_empty() || self.installing {
            return;
        }
        // a running server must not have its files replaced underneath it
        let keys: Vec<String> = components
            .iter()
            .enumerate()
            .filter(|(i, _)| self.selected.get(*i).copied().unwrap_or(false))
            .filter_map(|(_, c)| c.key)
            .map(str::to_string)
            .collect();
        let running: Vec<String> = self
            .modules
            .iter()
            .filter(|m| m.running() && keys.iter().any(|k| k == m.key))
            .map(|m| m.key.to_string())
            .collect();
        self.installing = true;
        self.update_output.clear();
        self.log("updates", format!("Installing: {}", chosen.join(", ")));
        if running.is_empty() {
            self.run_install(chosen);
        } else {
            let names: Vec<String> = self
                .modules
                .iter()
                .filter(|m| running.iter().any(|k| k == m.key))
                .map(|m| m.name.to_string())
                .collect();
            self.update_status = format!("Stopping {} first ...", names.join(", "));
            self.restart_after = running.clone();
            self.pending_install = chosen;
            self.start_modules("stop", running);
        }
    }

    fn run_install(&mut self, chosen: Vec<String>) {
        self.update_status = "Installing - please confirm the password window ...".into();
        let (program, args) = packages::install_command(&chosen);
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let output = std::process::Command::new(&program).args(&args).output();
            let (code, text) = match output {
                Ok(out) => (
                    out.status.code().unwrap_or(-1),
                    String::from_utf8_lossy(&out.stdout).to_string()
                        + &String::from_utf8_lossy(&out.stderr),
                ),
                Err(e) => (-1, format!("could not run {program}: {e}")),
            };
            let _ = tx.send(Msg::Installed(code, text));
        });
    }

    fn installed_updates(&mut self, code: i32, output: String) {
        self.installing = false;
        self.update_output = output;
        match code {
            0 => {
                self.update_status = "Updates installed.".into();
                self.log("updates", "Updates installed");
            }
            126 | 127 => {
                self.update_status = "Cancelled - nothing was changed.".into();
                self.error("updates", "Update cancelled - nothing was changed");
            }
            other => {
                self.update_status = format!("The package manager failed (exit code {other}).");
                self.error("updates", format!("Update failed (exit code {other})"));
            }
        }
        self.refresh_versions();
        let restart = std::mem::take(&mut self.restart_after);
        if !restart.is_empty() {
            self.start_modules("start", restart);
        }
        self.upgrades.clear();
        self.check_updates(false);
    }

    // ----------------------------------------------------------------- window
    fn show_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
    }

    /// The close button: the window really closes (Wayland cannot hide one),
    /// and the tray process keeps LAMPPost in the top bar.
    fn close(&mut self, ctx: &egui::Context) {
        if self.settings.minimize_to_tray() {
            crate::ensure_tray();
        } else {
            crate::quit_tray();
        }
        self.quitting = true;
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }

    fn apply_theme(&mut self, ctx: &egui::Context) {
        self.palette = theme::palette(self.look, self.scheme);
        theme::apply(ctx, &self.palette, self.look);
    }
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0] // the window frame is drawn, so the corners stay clear
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.settings.set_window_size(self.size);
        self.settings.save();
        theme::stop_watching();
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_messages(&ctx);

        // a maximised window would otherwise come back screen-sized but unmaximised
        if !edge_to_edge(&ctx)
            && let Some(size) = ctx.input(|i| i.viewport().inner_rect).map(|r| r.size())
        {
            self.size = [size.x, size.y];
        }

        // closing leaves the tray icon running, like XAMPP leaves its modules
        if ctx.input(|i| i.viewport().close_requested()) && !self.quitting {
            self.close(&ctx);
        }

        if Instant::now() >= self.next_refresh {
            self.refresh_modules();
            let interval = if Instant::now() < self.fast_until { 300 } else { 1500 };
            self.next_refresh = Instant::now() + Duration::from_millis(interval);
        }

        self.frame_and_window(ui);
        self.dialogs(&ctx);

        // background work (start/stop, updates, theme) wakes the window itself,
        // so idle it only needs to come back for the next status poll
        ctx.request_repaint_after(self.next_refresh.saturating_duration_since(Instant::now()));
    }
}

// ----------------------------------------------------------------- the window
impl App {
    /// The whole window: rounded frame, own title bar, then the contents.
    fn frame_and_window(&mut self, ui: &mut egui::Ui) {
        let (p, look) = (self.palette, self.look);
        let ctx = ui.ctx().clone();
        let rect = ui.max_rect();
        // maximised or fullscreen: square corners and no outline, so nothing
        // of the desktop shows through and no line runs along the screen edge
        let edge_to_edge = edge_to_edge(&ctx);
        let (radius, stroke) = if edge_to_edge {
            (0, egui::Stroke::NONE)
        } else if look == Look::Classic {
            (2, egui::Stroke::new(1.0, p.border))
        } else {
            (14, egui::Stroke::new(1.0, p.border))
        };

        ui.painter().rect(
            rect,
            egui::CornerRadius::same(radius),
            p.bg,
            stroke,
            egui::StrokeKind::Inside,
        );
        if !edge_to_edge {
            self.window_edges(&ctx, rect);
        }

        let mut content = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect.shrink(if edge_to_edge { 0.0 } else { 1.0 }))
                .layout(Layout::top_down(Align::Min)),
        );
        self.title_bar(&mut content);
        let inner = content.max_rect().shrink2(Vec2::new(13.0, 0.0));
        let inner = egui::Rect::from_min_max(
            egui::pos2(inner.min.x, content.cursor().min.y + 4.0),
            egui::pos2(inner.max.x, inner.max.y - 12.0),
        );
        let mut body = ui.new_child(
            egui::UiBuilder::new().max_rect(inner).layout(Layout::top_down(Align::Min)),
        );
        self.window(&mut body);
    }

    /// Dragging near an edge resizes the window (there is no system frame).
    fn window_edges(&mut self, ctx: &egui::Context, rect: egui::Rect) {
        use egui::ResizeDirection as Dir;
        let Some(pos) = ctx.pointer_latest_pos() else { return };
        let margin = 6.0;
        let (left, right) = (pos.x <= rect.left() + margin, pos.x >= rect.right() - margin);
        let (top, bottom) = (pos.y <= rect.top() + margin, pos.y >= rect.bottom() - margin);
        let direction = match (left, right, top, bottom) {
            (true, _, true, _) => Some((Dir::NorthWest, egui::CursorIcon::ResizeNwSe)),
            (_, true, true, _) => Some((Dir::NorthEast, egui::CursorIcon::ResizeNeSw)),
            (true, _, _, true) => Some((Dir::SouthWest, egui::CursorIcon::ResizeNeSw)),
            (_, true, _, true) => Some((Dir::SouthEast, egui::CursorIcon::ResizeNwSe)),
            (true, _, _, _) => Some((Dir::West, egui::CursorIcon::ResizeHorizontal)),
            (_, true, _, _) => Some((Dir::East, egui::CursorIcon::ResizeHorizontal)),
            (_, _, true, _) => Some((Dir::North, egui::CursorIcon::ResizeVertical)),
            (_, _, _, true) => Some((Dir::South, egui::CursorIcon::ResizeVertical)),
            _ => None,
        };
        let Some((direction, cursor)) = direction else { return };
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }

    /// Title bar in the app's own colours: logo, name, status and the three
    /// window buttons.
    fn title_bar(&mut self, ui: &mut egui::Ui) {
        let (p, look) = (self.palette, self.look);
        let ctx = ui.ctx().clone();
        let height = if look == Look::Classic { 40.0 } else { 52.0 };
        let (bar, response) = ui.allocate_exact_size(
            Vec2::new(ui.available_width(), height),
            egui::Sense::click_and_drag(),
        );
        if look == Look::Classic {
            ui.painter().rect_filled(bar, egui::CornerRadius::ZERO, p.card_alt);
            ui.painter().line_segment(
                [egui::pos2(bar.left(), bar.bottom()), egui::pos2(bar.right(), bar.bottom())],
                egui::Stroke::new(1.0, p.border),
            );
        }
        if response.drag_started_by(egui::PointerButton::Primary) {
            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
        if response.double_clicked() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
        }

        let mut bar_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(bar.shrink2(Vec2::new(12.0, 6.0)))
                .layout(Layout::left_to_right(Align::Center)),
        );
        bar_ui.spacing_mut().item_spacing.x = 10.0;
        logo(&mut bar_ui, height - 20.0);
        bar_ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            ui.label(
                RichText::new(format!("LAMPPost Control Panel v{}", env!("CARGO_PKG_VERSION")))
                    .font(FontId::new(if look == Look::Classic { 14.0 } else { 16.0 }, theme::bold()))
                    .color(p.text),
            );
            if look != Look::Classic {
                ui.label(
                    RichText::new("Apache · MariaDB · PHP · phpMyAdmin · FTP · Mail · Tomcat")
                        .size(11.0)
                        .color(p.muted),
                );
            }
        });
        bar_ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            self.window_buttons(ui, maximized);
            if look != Look::Classic {
                ui.add_space(8.0);
                let running: Vec<&str> =
                    self.modules.iter().filter(|m| m.running()).map(|m| m.name).collect();
                let text = if running.is_empty() {
                    "All modules stopped".to_string()
                } else {
                    format!("{} running:  {}", running.len(), running.join(", "))
                };
                egui::Frame::new()
                    .fill(p.accent_soft)
                    .corner_radius(egui::CornerRadius::same(13))
                    .inner_margin(egui::Margin::symmetric(13, 5))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(text)
                                .font(FontId::new(12.0, theme::semibold()))
                                .color(p.accent),
                        );
                    });
            }
        });
    }

    /// Minimise, maximise and close, drawn so they match the theme exactly.
    fn window_buttons(&mut self, ui: &mut egui::Ui, maximized: bool) {
        let p = self.palette;
        let ctx = ui.ctx().clone();
        let size = Vec2::splat(28.0);
        for kind in ["close", "maximize", "minimize"] {
            let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click());
            let hovered = response.hovered();
            let fill = if kind == "close" && hovered {
                p.red_btn
            } else if hovered {
                p.hover_fill()
            } else {
                egui::Color32::TRANSPARENT
            };
            let painter = ui.painter();
            painter.rect_filled(rect, egui::CornerRadius::same(14), fill);
            let colour = if kind == "close" && hovered { p.on_color } else { p.text };
            let stroke = egui::Stroke::new(1.4, colour);
            let c = rect.center();
            let d = 4.5;
            match kind {
                "minimize" => {
                    painter.line_segment(
                        [egui::pos2(c.x - d, c.y + 3.0), egui::pos2(c.x + d, c.y + 3.0)],
                        stroke,
                    );
                }
                "maximize" => {
                    let square = egui::Rect::from_center_size(c, Vec2::splat(d * 2.0));
                    painter.rect_stroke(square, egui::CornerRadius::same(2), stroke, egui::StrokeKind::Inside);
                    if maximized {
                        let offset = Vec2::new(2.5, -2.5);
                        painter.rect_stroke(
                            egui::Rect::from_center_size(c + offset, Vec2::splat(d * 2.0)),
                            egui::CornerRadius::same(2),
                            egui::Stroke::new(1.0, p.muted),
                            egui::StrokeKind::Inside,
                        );
                    }
                }
                _ => {
                    painter.line_segment([c + Vec2::new(-d, -d), c + Vec2::new(d, d)], stroke);
                    painter.line_segment([c + Vec2::new(-d, d), c + Vec2::new(d, -d)], stroke);
                }
            }
            if response.clicked() {
                match kind {
                    "minimize" => ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true)),
                    "maximize" => ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized)),
                    _ => {
                        let _ = self.tx.send(Msg::CloseWindow);
                    }
                }
            }
        }
    }

    fn window(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let look = self.look;
        ui.spacing_mut().item_spacing = Vec2::new(10.0, 10.0);

        // modules and sidebar
        ui.horizontal_top(|ui| {
            let width = (ui.available_width() - SIDEBAR - 10.0).max(400.0);
            ui.allocate_ui_with_layout(
                Vec2::new(width, 0.0),
                Layout::top_down(Align::Min),
                |ui| {
                    card(ui, &p, look, |ui| {
                        ui.set_width(ui.available_width());
                        caption(ui, &p, "MODULES");
                        ui.add_space(2.0);
                        self.module_rows(ui);
                    });
                },
            );
            ui.allocate_ui_with_layout(
                Vec2::new(SIDEBAR, 0.0),
                Layout::top_down_justified(Align::Min),
                |ui| self.sidebar(ui),
            );
        });

        // log
        card(ui, &p, look, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                caption(ui, &p, "LOG");
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if ui.add(egui::Button::new(RichText::new("Clear").size(12.0).color(p.muted)).frame(false)).clicked() {
                        self.log.clear();
                    }
                });
            });
            ui.add_space(4.0);
            let height = (ui.available_height() - 6.0).max(80.0);
            // up to 500 lines, of which only a dozen are visible: lay out just
            // those (one line each, long ones are shortened with a tooltip)
            let font = FontId::monospace(11.5);
            // show_rows places rows with the spacing of the ui it is given, so
            // set the log's line spacing here rather than inside the rows
            ui.spacing_mut().item_spacing.y = 2.0;
            let row_height = ui.fonts_mut(|f| f.row_height(&font));
            egui::ScrollArea::vertical()
                .stick_to_bottom(true)
                .max_height(height)
                .auto_shrink([false, false])
                .show_rows(ui, row_height, self.log.len(), |ui, rows| {
                    for line in &self.log[rows] {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 6.0;
                            ui.label(RichText::new(&line.time).font(font.clone()).color(p.muted));
                            ui.label(
                                RichText::new(format!("[{}]", line.source))
                                    .font(font.clone())
                                    .color(if line.error { p.red } else { p.blue }),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(&line.text)
                                        .font(font.clone())
                                        .color(if line.error { p.red } else { p.text }),
                                )
                                .truncate(),
                            );
                        });
                    }
                });
        });
    }

    fn module_rows(&mut self, ui: &mut egui::Ui) {
        let (p, look) = (self.palette, self.look);
        let mut clicked: Option<String> = None;
        let mut admin: Option<usize> = None;
        let mut open: Option<std::path::PathBuf> = None;

        for index in 0..self.modules.len() {
            if index > 0 {
                ui.add_space(2.0);
                let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 0.0, p.border);
                ui.add_space(2.0);
            }
            let module = &self.modules[index];
            let (key, name, info, engine) = (
                module.key.to_string(),
                module.name.to_string(),
                module.info().to_string(),
                module.engine.to_string(),
            );
            let (running, busy) = (module.running(), module.busy);
            let pid_text = if running {
                // the main PID, plus how many workers it has (Apache has several)
                match module.pids.len() {
                    1 => module.pids[0].to_string(),
                    n => format!("{} +{}", module.pids[0], n - 1),
                }
            } else if look == Look::Classic {
                String::new()
            } else {
                "-".into()
            };
            let port_text = if running {
                module.ports.iter().map(u16::to_string).collect::<Vec<_>>().join(", ")
            } else if look == Look::Classic {
                String::new()
            } else {
                "-".into()
            };
            let configs: Vec<(String, std::path::PathBuf)> =
                module.configs.iter().map(|e| (e.label.to_string(), e.target.clone())).collect();
            let logs: Vec<(String, std::path::PathBuf)> =
                module.logs.iter().map(|e| (e.label.to_string(), e.target.clone())).collect();

            // one row of fixed height, so every column lines up
            ui.allocate_ui_with_layout(
                Vec2::new(ui.available_width(), 44.0),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    dot(ui, &p, running, busy);
                    let fixed = 76.0 + 84.0 + 84.0 + 80.0 + 86.0 + 80.0 + 6.0 * 8.0;
                    let name_width = (ui.available_width() - fixed).max(90.0);
                    // allocate the space first: a short name must not shrink the column
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(name_width, 40.0), egui::Sense::hover());
                    let mut text_ui = ui.new_child(
                        egui::UiBuilder::new().max_rect(rect).layout(Layout::top_down(Align::Min)),
                    );
                    text_ui.spacing_mut().item_spacing.y = 1.0;
                    text_ui
                        .add(
                            egui::Label::new(
                                RichText::new(&name).font(FontId::new(15.0, theme::bold())).color(p.text),
                            )
                            .truncate(),
                        )
                        .on_hover_text(&engine);
                    if look != Look::Classic {
                        text_ui
                            .add(
                                egui::Label::new(RichText::new(&info).size(11.5).color(p.muted)).truncate(),
                            )
                            .on_hover_text(&engine);
                    }

                    chip(ui, &p, look, &pid_text, running, 76.0);
                    chip(ui, &p, look, &port_text, running, 84.0);

                    let fill = if running { p.red_btn } else { p.green_btn };
                    let label = if running { "Stop" } else { "Start" };
                    if filled_button(ui, &p, look, label, fill, 80.0).clicked() && !busy {
                        clicked = Some(key.clone());
                    }
                    if ui
                        .add_enabled(running, egui::Button::new("Admin").min_size(Vec2::new(80.0, 30.0)))
                        .clicked()
                    {
                        admin = Some(index);
                    }
                    let config = plain_button(ui, "Config", 86.0);
                    egui::Popup::menu(&config).show(|ui| {
                        ui.set_min_width(240.0);
                        for (label, target) in &configs {
                            if ui.button(label).clicked() {
                                open = Some(target.clone());
                                ui.close();
                            }
                        }
                    });
                    let log_button = plain_button(ui, "Logs", 80.0);
                    egui::Popup::menu(&log_button).show(|ui| {
                        ui.set_min_width(240.0);
                        for (label, target) in &logs {
                            if ui.button(label).clicked() {
                                open = Some(target.clone());
                                ui.close();
                            }
                        }
                    });
                },
            );
        }

        if let Some(key) = clicked {
            self.toggle(&key);
        }
        if let Some(index) = admin {
            let (name, target) = {
                let module = &self.modules[index];
                (module.name.to_string(), match &module.admin {
                    Admin::Url(url) => url.to_string(),
                    Admin::Dir(dir) => dir.display().to_string(),
                })
            };
            self.log(&name, format!("Opening {target}"));
            let opened = match &self.modules[index].admin {
                Admin::Url(url) => cli::open_url(url),
                Admin::Dir(dir) => cli::open_path(dir),
            };
            if !opened {
                self.error(&name, "Could not open it - is xdg-open installed?");
            }
        }
        if let Some(target) = open {
            self.open_entry(&target);
        }
    }

    fn sidebar(&mut self, ui: &mut egui::Ui) {
        let updates_label = if self.updates_available > 0 { "Updates ●" } else { "Updates" };
        let buttons: [(&str, Dialog); 5] = [
            ("Config", Dialog::Config),
            (updates_label, Dialog::Updates),
            ("Netstat", Dialog::Netstat),
            ("Services", Dialog::Services),
            ("Help", Dialog::About),
        ];
        for (label, dialog) in buttons {
            if ui.add(egui::Button::new(label).min_size(Vec2::new(SIDEBAR, 34.0))).clicked() {
                self.dialog = dialog;
                match dialog {
                    Dialog::Config => self.editor_edit = self.settings.editor(),
                    Dialog::Updates => {
                        self.refresh_versions();
                        if self.upgrades.is_empty() && !self.checking {
                            self.check_updates(false);
                        }
                    }
                    Dialog::Netstat => self.sockets = sysinfo::listening(),
                    Dialog::Services => self.services = system_services(),
                    _ => {}
                }
            }
        }
        if ui.add(egui::Button::new("Shell").min_size(Vec2::new(SIDEBAR, 34.0))).clicked() {
            if cli::open_terminal(&settings::prefix()) {
                self.log("main", format!("Opening a shell in {}", settings::prefix().display()));
            } else {
                self.error("main", "No terminal program found");
            }
        }
        if ui.add(egui::Button::new("Explorer").min_size(Vec2::new(SIDEBAR, 34.0))).clicked() {
            cli::open_path(&settings::prefix());
        }
        if ui.add(egui::Button::new("Quit").min_size(Vec2::new(SIDEBAR, 34.0))).clicked() {
            let _ = self.tx.send(Msg::Quit);
        }
    }
}

/// Maximised or fullscreen: the window touches the screen edges.
fn edge_to_edge(ctx: &egui::Context) -> bool {
    ctx.input(|i| {
        let viewport = i.viewport();
        viewport.maximized.unwrap_or(false) || viewport.fullscreen.unwrap_or(false)
    })
}

/// The distribution's own services that would compete for LAMPPost's ports.
/// Units this distribution does not have (apache2 on Fedora, httpd on
/// Debian) are left out. Rows: (name, unit, active state, enabled at boot).
fn system_services() -> Vec<(String, String, String, String)> {
    let units = [
        ("Apache", "httpd"),
        ("Apache", "apache2"),
        ("PHP-FPM", "php-fpm"),
        ("MariaDB", "mariadb"),
        ("Tomcat", "tomcat"),
        ("Tomcat", "tomcat10"),
    ];
    units
        .iter()
        .filter_map(|(name, unit)| {
            let enabled = sysinfo::systemctl("is-enabled", unit);
            if enabled == "not-found" || enabled == "unknown" {
                return None;
            }
            let active = sysinfo::systemctl("is-active", unit);
            Some((name.to_string(), format!("{unit}.service"), active, enabled))
        })
        .collect()
}

// ---------------------------------------------------------------- the dialogs
impl App {
    fn dialogs(&mut self, ctx: &egui::Context) {
        match self.dialog {
            Dialog::None => {}
            Dialog::Config => self.config_dialog(ctx),
            Dialog::Updates => self.updates_dialog(ctx),
            Dialog::Netstat => self.netstat_dialog(ctx),
            Dialog::Services => self.services_dialog(ctx),
            Dialog::About => self.about_dialog(ctx),
            Dialog::Install => self.install_dialog(ctx),
        }
    }

    fn modal<R>(&mut self, ctx: &egui::Context, id: &str, width: f32, add: impl FnOnce(&mut Self, &mut egui::Ui) -> R) {
        let p = self.palette;
        let response = egui::Modal::new(egui::Id::new(id))
            .frame(
                egui::Frame::new()
                    .fill(p.card)
                    .stroke(egui::Stroke::new(1.0, p.border))
                    .corner_radius(egui::CornerRadius::same(if self.look == Look::Classic { 3 } else { 16 }))
                    .inner_margin(egui::Margin::same(18)),
            )
            .show(ctx, |ui| {
                ui.set_width(width);
                add(self, ui);
            });
        let busy = self.dialog == Dialog::Install && self.setup_state == Setup::Installing;
        if response.should_close() && !busy {
            self.dialog = Dialog::None;
        }
    }

    fn config_dialog(&mut self, ctx: &egui::Context) {
        self.modal(ctx, "config", 640.0, |app, ui| {
            let p = app.palette;
            heading(ui, &p, "Configuration");
            muted(ui, &p, "Changes apply immediately and are stored in lamppost-control.ini.");
            ui.spacing_mut().item_spacing.y = 0.0;
            // heading, buttons and margins take about 190 px of the window
            let max = (ui.ctx().content_rect().height() - 190.0).max(240.0);
            egui::ScrollArea::vertical().max_height(max).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;

                section(ui, &p, "APPEARANCE");
                let (mut look, mut scheme) = (app.look, app.scheme);
                setting_row(ui, &p, 0, "Look", "", |ui| {
                    segmented(ui, &p, &mut look, &[(Look::Modern, "Modern"), (Look::Classic, "Classic")], true);
                });
                setting_row(ui, &p, 1, "Colours", "Modern look only; System follows the desktop", |ui| {
                    segmented(
                        ui,
                        &p,
                        &mut scheme,
                        &[(Scheme::System, "System"), (Scheme::Light, "Light"), (Scheme::Dark, "Dark")],
                        look == Look::Modern,
                    );
                });
                if look != app.look || scheme != app.scheme {
                    app.look = look;
                    app.scheme = scheme;
                    app.settings.set_look(look);
                    app.settings.set_scheme(scheme);
                    app.settings.save();
                    app.apply_theme(ui.ctx());
                }

                section(ui, &p, "EDITOR");
                setting_row(ui, &p, 0, "Editor command", "Opens the files behind Config and Logs", |ui| {
                    let response = ui.add(
                        egui::TextEdit::singleline(&mut app.editor_edit)
                            .desired_width(240.0)
                            .font(FontId::monospace(12.5))
                            .hint_text("gnome-text-editor"),
                    );
                    if response.lost_focus() {
                        app.settings.set_editor(&app.editor_edit.clone());
                        app.settings.save();
                    }
                });

                section(ui, &p, "AUTOSTART OF MODULES");
                let modules: Vec<(&'static str, &'static str)> = settings::MODULE_KEYS
                    .iter()
                    .filter_map(|key| app.modules.iter().find(|m| m.key == *key))
                    .map(|m| (m.key, m.name))
                    .collect();
                // the section title says it all: just the five modules in one row
                let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), egui::Sense::hover());
                let mut line = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row.shrink2(Vec2::new(10.0, 0.0)))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                square_ticks(&mut line);
                line.spacing_mut().item_spacing.x = 18.0;
                for (key, name) in &modules {
                    let mut value = app.settings.autostart(key);
                    if line.checkbox(&mut value, *name).changed() {
                        app.settings.set_autostart(key, value);
                        app.settings.save();
                    }
                }

                section(ui, &p, "GENERAL");
                let mut ports = app.settings.check_ports();
                setting_row(ui, &p, 0, "Check the default ports at startup", "Reports conflicts before you press Start", |ui| {
                    square_ticks(ui);
                    if ui.checkbox(&mut ports, "").changed() {
                        app.settings.set_check_ports(ports);
                        app.settings.save();
                    }
                });
                let mut updates = app.settings.check_updates();
                setting_row(ui, &p, 1, "Look for new versions at startup", "Queries the package manager, no password needed", |ui| {
                    square_ticks(ui);
                    if ui.checkbox(&mut updates, "").changed() {
                        app.settings.set_check_updates(updates);
                        app.settings.save();
                    }
                });
                let mut minimize = app.settings.minimize_to_tray();
                setting_row(ui, &p, 2, "Keep running in the tray when the window closes", "", |ui| {
                    square_ticks(ui);
                    if ui.checkbox(&mut minimize, "").changed() {
                        app.settings.set_minimize_to_tray(minimize);
                        app.settings.save();
                    }
                });
                let mut login = settings::login_autostart();
                let mut failed = None;
                setting_row(ui, &p, 3, "Start the tray icon at login", "Adds an entry to ~/.config/autostart", |ui| {
                    square_ticks(ui);
                    if ui.checkbox(&mut login, "").changed()
                        && let Err(e) = settings::set_login_autostart(login)
                    {
                        failed = Some(e.to_string());
                    }
                });
                if let Some(e) = failed {
                    app.error("main", format!("Autostart: {e}"));
                }
            });

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                if plain_button(ui, "Versions and Updates ...", 200.0).clicked() {
                    app.dialog = Dialog::Updates;
                    app.refresh_versions();
                    if app.upgrades.is_empty() && !app.checking {
                        app.check_updates(false);
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if plain_button(ui, "Close", 90.0).clicked() {
                        app.dialog = Dialog::None;
                    }
                });
            });
        });
    }

    fn updates_dialog(&mut self, ctx: &egui::Context) {
        self.modal(ctx, "updates", 760.0, |app, ui| {
            let p = app.palette;
            heading(ui, &p, "Versions and Updates");
            let components = packages::components();
            let available = app.selected_len_available();
            let checked = match &app.checked_at {
                Some(time) => format!("last checked {time}"),
                None => "not checked yet".to_string(),
            };
            muted(
                ui,
                &p,
                &format!("{} components  ·  {available} with updates  ·  {checked}", components.len()),
            );
            ui.add_space(10.0);

            // [x] | Component | Package | Installed | Available
            let widths = [28.0, 0.0, 170.0, 100.0, 130.0];
            let flexible = (ui.available_width() - widths.iter().sum::<f32>() - 4.0 * 8.0).max(140.0);
            let width = |i: usize| if i == 1 { flexible } else { widths[i] };
            let mono = FontId::monospace(12.5);

            ui.spacing_mut().item_spacing.y = 3.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for (i, title) in ["", "COMPONENT", "PACKAGE", "INSTALLED", "AVAILABLE"].iter().enumerate() {
                    cell(ui, width(i), i == 3, |ui| {
                        ui.label(RichText::new(*title).font(FontId::new(11.0, theme::semibold())).color(p.muted));
                    });
                }
            });
            let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
            ui.painter().rect_filled(line, 0.0, p.border);

            ui.spacing_mut().item_spacing.y = 0.0;
            for (index, component) in components.iter().enumerate() {
                let new: Option<String> = component
                    .packages
                    .iter()
                    .find_map(|name| app.upgrades.iter().find(|(p, _)| p == name).map(|(_, v)| v.clone()));
                let installed = app
                    .installed
                    .iter()
                    .find(|(name, _)| name == component.main)
                    .map(|(_, v)| packages::short_version(v));

                let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), egui::Sense::hover());
                if index % 2 == 1 {
                    ui.painter().rect_filled(row, egui::CornerRadius::same(6), p.card_alt);
                }
                let mut line = ui.new_child(
                    egui::UiBuilder::new().max_rect(row).layout(Layout::left_to_right(Align::Center)),
                );
                line.spacing_mut().item_spacing.x = 8.0;
                let ui = &mut line;

                cell(ui, width(0), false, |ui| {
                    square_ticks(ui);
                    let mut selected = app.selected.get(index).copied().unwrap_or(false);
                    ui.add_enabled_ui(new.is_some(), |ui| {
                        if ui.checkbox(&mut selected, "").changed()
                            && let Some(slot) = app.selected.get_mut(index)
                        {
                            *slot = selected;
                        }
                    });
                });
                cell(ui, width(1), false, |ui| {
                    ui.label(RichText::new(component.title).color(p.text))
                        .on_hover_text(component.packages.join(", "));
                });
                cell(ui, width(2), false, |ui| {
                    ui.label(RichText::new(component.main).font(mono.clone()).color(p.muted));
                });
                cell(ui, width(3), true, |ui| match &installed {
                    Some(version) => {
                        ui.label(RichText::new(version).font(mono.clone()).color(p.text));
                    }
                    None => {
                        tag(ui, "not installed", p.red, p.red_soft);
                    }
                });
                cell(ui, width(4), false, |ui| match &new {
                    Some(version) => {
                        tag(ui, &packages::short_version(version), p.accent, p.accent_soft)
                            .on_hover_text(format!("New version: {version}"));
                    }
                    None if app.checked_at.is_some() => {
                        tag(ui, "up to date", p.muted, p.border);
                    }
                    None => {
                        ui.label(RichText::new("—").color(p.faint));
                    }
                });
            }

            ui.spacing_mut().item_spacing.y = 6.0;
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if app.checking || app.installing {
                    ui.add(egui::Spinner::new());
                }
                muted(ui, &p, &app.update_status.clone());
            });
            if !app.update_output.is_empty() {
                egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                    ui.label(RichText::new(&app.update_output).monospace().size(11.0).color(p.muted));
                });
            }
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                let busy = app.checking || app.installing;
                if ui
                    .add_enabled(!busy, egui::Button::new("Check for updates").min_size(Vec2::new(170.0, 30.0)))
                    .clicked()
                {
                    app.check_updates(true);
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if plain_button(ui, "Close", 90.0).clicked() {
                        app.dialog = Dialog::None;
                    }
                    let any = app.selected.iter().any(|s| *s);
                    ui.add_enabled_ui(any && !busy, |ui| {
                        if filled_button(ui, &p, app.look, "Install selected updates", p.accent_btn, 200.0).clicked() {
                            app.install_updates();
                        }
                    });
                });
            });
        });
    }

    /// How many components have an update waiting.
    fn selected_len_available(&self) -> usize {
        packages::components()
            .iter()
            .filter(|c| c.packages.iter().any(|name| self.upgrades.iter().any(|(p, _)| p == name)))
            .count()
    }

    fn netstat_dialog(&mut self, ctx: &egui::Context) {
        self.modal(ctx, "netstat", 800.0, |app, ui| {
            let p = app.palette;
            heading(ui, &p, "Listening TCP sockets");

            // which module a port belongs to, and whether that module holds it
            let owner = |socket: &sysinfo::Socket| -> Option<(&'static str, bool)> {
                let module = app.modules.iter().find(|m| m.ports.contains(&socket.port))?;
                let ours = socket.pid.parse::<i32>().is_ok_and(|pid| module.pids.contains(&pid));
                Some((module.name, ours))
            };
            let total = app.sockets.len();
            let lamppost = app.sockets.iter().filter(|s| owner(s).is_some()).count();
            let exposed = app.sockets.iter().filter(|s| s.scope != sysinfo::Scope::Local).count();
            muted(
                ui,
                &p,
                &format!("{total} sockets  ·  {lamppost} on LAMPPost ports  ·  {exposed} reachable from the network"),
            );
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                square_ticks(ui);
                ui.checkbox(&mut app.netstat_only_ours, "Only LAMPPost ports");
            });
            ui.add_space(8.0);

            // Port | Address | Scope | Process | PID | LAMPPost
            let widths = [58.0, 150.0, 118.0, 0.0, 70.0, 150.0];
            let flexible = (ui.available_width() - widths.iter().sum::<f32>() - 5.0 * 8.0).max(120.0);
            let width = |i: usize| if i == 3 { flexible } else { widths[i] };
            let mono = FontId::monospace(12.5);

            // the header stays put while the rows scroll
            ui.spacing_mut().item_spacing.y = 3.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for (i, title) in ["PORT", "ADDRESS", "SCOPE", "PROCESS", "PID", "LAMPPOST"].iter().enumerate() {
                    cell(ui, width(i), i == 0 || i == 4, |ui| {
                        ui.label(RichText::new(*title).font(FontId::new(11.0, theme::semibold())).color(p.muted));
                    });
                }
            });
            let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
            ui.painter().rect_filled(line, 0.0, p.border);

            let rows: Vec<&sysinfo::Socket> = app
                .sockets
                .iter()
                .filter(|s| !app.netstat_only_ours || owner(s).is_some())
                .collect();
            egui::ScrollArea::vertical().max_height(330.0).auto_shrink([false, true]).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if rows.is_empty() {
                    ui.add_space(12.0);
                    muted(ui, &p, "No listening sockets on LAMPPost ports.");
                }
                for (index, socket) in rows.iter().enumerate() {
                    let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), egui::Sense::hover());
                    if index % 2 == 1 {
                        ui.painter().rect_filled(row, egui::CornerRadius::same(6), p.card_alt);
                    }
                    let mut line = ui.new_child(
                        egui::UiBuilder::new().max_rect(row).layout(Layout::left_to_right(Align::Center)),
                    );
                    line.spacing_mut().item_spacing.x = 8.0;
                    let ui = &mut line;

                    cell(ui, width(0), true, |ui| {
                        ui.label(RichText::new(socket.port.to_string()).font(mono.clone()).strong().color(p.text));
                    });
                    cell(ui, width(1), false, |ui| {
                        ui.add(egui::Label::new(RichText::new(&socket.address).font(mono.clone()).color(p.text)).truncate());
                    });
                    cell(ui, width(2), false, |ui| match socket.scope {
                        sysinfo::Scope::Local => {
                            tag(ui, "local", p.muted, p.border).on_hover_text("Loopback only: reachable from this computer");
                        }
                        sysinfo::Scope::All => {
                            tag(ui, "all interfaces", p.accent, p.accent_soft)
                                .on_hover_text("Bound to every interface: reachable from the network");
                        }
                        sysinfo::Scope::Network => {
                            tag(ui, "network", p.accent, p.accent_soft)
                                .on_hover_text("Bound to a network address: reachable from the network");
                        }
                    });
                    cell(ui, width(3), false, |ui| {
                        if socket.name.is_empty() {
                            ui.label(RichText::new("—").color(p.faint))
                                .on_hover_text("Owned by another user (for example root); not visible to you");
                        } else {
                            ui.add(egui::Label::new(RichText::new(&socket.name).color(p.text)).truncate());
                        }
                    });
                    cell(ui, width(4), true, |ui| {
                        let text = if socket.pid.is_empty() { "—" } else { socket.pid.as_str() };
                        let colour = if socket.pid.is_empty() { p.faint } else { p.text };
                        ui.label(RichText::new(text).font(mono.clone()).color(colour));
                    });
                    cell(ui, width(5), false, |ui| match owner(socket) {
                        Some((name, true)) => {
                            tag(ui, name, p.green, p.green_soft);
                        }
                        Some((name, false)) => {
                            tag(ui, &format!("blocks {name}"), p.red, p.red_soft)
                                .on_hover_text(format!("Another program holds a port {name} needs"));
                        }
                        None => {}
                    });
                }
            });

            ui.add_space(12.0);
            ui.horizontal(|ui| {
                if plain_button(ui, "Refresh", 110.0).clicked() {
                    app.sockets = sysinfo::listening();
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if plain_button(ui, "Close", 90.0).clicked() {
                        app.dialog = Dialog::None;
                    }
                });
            });
        });
    }

    fn services_dialog(&mut self, ctx: &egui::Context) {
        self.modal(ctx, "services", 640.0, |app, ui| {
            let p = app.palette;
            heading(ui, &p, "System services");
            muted(ui, &p, "LAMPPost runs its own copies.");
            ui.add_space(10.0);

            // Service | Unit | State | At boot
            let widths = [0.0, 180.0, 110.0, 110.0];
            let flexible = (ui.available_width() - widths.iter().sum::<f32>() - 3.0 * 8.0).max(120.0);
            let width = |i: usize| if i == 0 { flexible } else { widths[i] };

            ui.spacing_mut().item_spacing.y = 3.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                for (i, title) in ["SERVICE", "UNIT", "STATE", "AT BOOT"].iter().enumerate() {
                    cell(ui, width(i), false, |ui| {
                        ui.label(RichText::new(*title).font(FontId::new(11.0, theme::semibold())).color(p.muted));
                    });
                }
            });
            let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), egui::Sense::hover());
            ui.painter().rect_filled(line, 0.0, p.border);

            ui.spacing_mut().item_spacing.y = 0.0;
            if app.services.is_empty() {
                ui.add_space(12.0);
                muted(ui, &p, "None of these services is installed.");
            }
            let mono = FontId::monospace(12.5);
            for (index, (name, unit, active, enabled)) in app.services.iter().enumerate() {
                let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), egui::Sense::hover());
                if index % 2 == 1 {
                    ui.painter().rect_filled(row, egui::CornerRadius::same(6), p.card_alt);
                }
                let mut line = ui.new_child(
                    egui::UiBuilder::new().max_rect(row).layout(Layout::left_to_right(Align::Center)),
                );
                line.spacing_mut().item_spacing.x = 8.0;
                let ui = &mut line;
                cell(ui, width(0), false, |ui| {
                    ui.label(RichText::new(name).color(p.text));
                });
                cell(ui, width(1), false, |ui| {
                    ui.label(RichText::new(unit).font(mono.clone()).color(p.muted));
                });
                cell(ui, width(2), false, |ui| {
                    // a running system service holds the ports LAMPPost needs
                    if active == "active" {
                        tag(ui, "active", p.red, p.red_soft)
                            .on_hover_text("Running: it occupies the same ports as the LAMPPost module");
                    } else {
                        tag(ui, active, p.muted, p.border);
                    }
                });
                cell(ui, width(3), false, |ui| {
                    if enabled == "enabled" {
                        tag(ui, "enabled", p.accent, p.accent_soft)
                            .on_hover_text("Starts at boot and will take the ports again after a restart");
                    } else {
                        tag(ui, enabled, p.muted, p.border);
                    }
                });
            }

            ui.add_space(14.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if plain_button(ui, "Close", 90.0).clicked() {
                    app.dialog = Dialog::None;
                }
            });
        });
    }

    fn install_dialog(&mut self, ctx: &egui::Context) {
        self.modal(ctx, "install", 560.0, |app, ui| {
            let p = app.palette;
            let names = app.setup_names();
            let manager = match packages::distro() {
                packages::Distro::Deb => "apt",
                _ => "dnf",
            };
            let setup_only = app.setup_packages.is_empty() && app.setup_state != Setup::Checking;
            if setup_only {
                heading(ui, &p, &format!("{names} is not set up"));
                muted(
                    ui,
                    &p,
                    &format!("The packages are installed, but {names} has no configuration in {} yet.", settings::prefix().display()),
                );
            } else {
                heading(ui, &p, &format!("{names} is not installed"));
                muted(
                    ui,
                    &p,
                    "LAMPPost runs your distribution's own packages. These are required and will be installed:",
                );
            }
            ui.add_space(10.0);

            match &app.setup_state {
                Setup::Checking => {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        muted(ui, &p, "Querying the package database ...");
                    });
                }
                _ if !setup_only => {
                    ui.horizontal_wrapped(|ui| {
                        ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                        for package in &app.setup_packages {
                            tag(ui, package, p.text, p.border);
                        }
                    });
                }
                _ => {}
            }
            ui.add_space(12.0);

            match &app.setup_state {
                Setup::Installing => {
                    ui.horizontal(|ui| {
                        ui.add(egui::Spinner::new());
                        muted(ui, &p, &format!("Running {manager} - confirm the authentication prompt ..."));
                    });
                }
                Setup::Cancelled => {
                    ui.label(RichText::new("Authentication was not completed. Nothing was installed.").color(p.red));
                }
                Setup::Failed(output) => {
                    ui.label(RichText::new("The installation failed:").color(p.red));
                    egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                        ui.label(RichText::new(output).monospace().size(11.0).color(p.muted));
                    });
                }
                _ if setup_only => {
                    muted(
                        ui,
                        &p,
                        &format!("Setting it up may need administrator rights; your system then asks for your password once. {names} starts automatically afterwards."),
                    );
                }
                _ => {
                    muted(
                        ui,
                        &p,
                        &format!("Installing requires administrator rights: {manager} runs through pkexec and your system asks for your password once. {names} starts automatically afterwards."),
                    );
                }
            }

            ui.add_space(14.0);
            ui.horizontal(|ui| {
                let busy = matches!(app.setup_state, Setup::Checking | Setup::Installing);
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.add_enabled_ui(!busy, |ui| {
                        let label = match (&app.setup_state, setup_only) {
                            (Setup::Installing, _) => "Installing ...",
                            (Setup::Checking, _) | (Setup::Ready, false) => "Install and start",
                            (Setup::Ready, true) => "Set up and start",
                            _ => "Try again",
                        };
                        if filled_button(ui, &p, app.look, label, p.accent_btn, 150.0).clicked() {
                            app.run_setup_install();
                        }
                    });
                    ui.add_enabled_ui(!matches!(app.setup_state, Setup::Installing), |ui| {
                        if plain_button(ui, "Cancel", 90.0).clicked() {
                            app.dialog = Dialog::None;
                            app.setup_state = Setup::Checking;
                        }
                    });
                });
            });
        });
    }

    fn about_dialog(&mut self, ctx: &egui::Context) {
        self.modal(ctx, "about", 640.0, |app, ui| {
            let p = app.palette;
            ui.horizontal(|ui| {
                logo(ui, 52.0);
                ui.add_space(6.0);
                ui.vertical(|ui| {
                    heading(ui, &p, &format!("LAMPPost {}", env!("CARGO_PKG_VERSION")));
                    muted(ui, &p, "An XAMPP-style local web stack, built on your distribution's packages.");
                });
            });

            ui.spacing_mut().item_spacing.y = 0.0;
            section(ui, &p, "MODULES");
            let widths = [120.0, 0.0, 110.0];
            let flexible = (ui.available_width() - widths.iter().sum::<f32>() - 2.0 * 8.0).max(160.0);
            let width = |i: usize| if i == 1 { flexible } else { widths[i] };
            let mono = FontId::monospace(12.5);
            let rows = [
                ("Apache", "Apache HTTP Server + PHP-FPM", "80, 443"),
                ("MariaDB", "MariaDB server, with phpMyAdmin", "3306"),
                ("FTP Server", "pyftpdlib, accounts in lamppost-ftp.ini", "21"),
                ("Mail Catcher", "aiosmtpd, saves mail to mailoutput/", "25"),
                ("Tomcat", "Apache Tomcat", "8080, 8005"),
            ];
            for (index, (name, what, ports)) in rows.iter().enumerate() {
                let (row, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), egui::Sense::hover());
                if index % 2 == 1 {
                    ui.painter().rect_filled(row, egui::CornerRadius::same(6), p.card_alt);
                }
                let mut line = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row.shrink2(Vec2::new(10.0, 0.0)))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                line.spacing_mut().item_spacing.x = 8.0;
                let ui = &mut line;
                cell(ui, width(0), false, |ui| {
                    ui.label(RichText::new(*name).font(FontId::new(13.5, theme::semibold())).color(p.text));
                });
                cell(ui, width(1), false, |ui| {
                    ui.label(RichText::new(*what).color(p.muted));
                });
                cell(ui, width(2) - 20.0, true, |ui| {
                    ui.label(RichText::new(*ports).font(mono.clone()).color(p.text));
                });
            }

            section(ui, &p, "COMMAND LINE");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!(
                        "{} start|stop|restart|status|missing|install",
                        settings::path("lamppost").display()
                    ))
                    .font(mono.clone())
                    .color(p.text),
                );
            });

            section(ui, &p, "LICENCE");
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                muted(ui, &p, "MIT licensed. Not affiliated with Apache Friends or the XAMPP project.");
            });

            ui.add_space(16.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if plain_button(ui, "Close", 90.0).clicked() {
                    app.dialog = Dialog::None;
                }
            });
        });
    }
}
