//! LAMPPost Control Panel - start and stop your local web stack.
//!
//! All start/stop logic lives in the "lamppost" shell script next to this
//! program; the panel calls it and watches PID files and ports.
//!
//! Two processes, on purpose: the tray icon runs on its own (`--tray`) and
//! the window is started and closed as needed. Wayland does not let a window
//! hide itself, so "close to the tray" has to mean "really close the window
//! and leave the tray running" - which is also much lighter, because the tray
//! process needs no graphics at all.
mod app;
mod cli;
mod ini;
mod modules;
mod packages;
mod settings;
mod sysinfo;
mod theme;
mod tray;
mod widgets;

use app::Msg;
use eframe::egui;
use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::Duration;

static CTX: OnceLock<egui::Context> = OnceLock::new();

pub fn wake_ui() {
    if let Some(ctx) = CTX.get() {
        ctx.request_repaint();
    }
}

const HELP: &str = "\
LAMPPost Control Panel

Usage:
  lamppost-control                     open the control panel
  lamppost-control --tray              only the tray icon, no window (used at login)
  lamppost-control --install <module>  offer to install a module's missing packages

Starting and stopping also works without the panel:
  lamppost start|stop|status [module...]
";

// ---------------------------------------------------------------- the sockets
fn socket_path(name: &str) -> PathBuf {
    match std::env::var_os("XDG_RUNTIME_DIR") {
        Some(dir) => PathBuf::from(dir).join(format!("lamppost-{name}.sock")),
        None => {
            let user = std::env::var("USER").unwrap_or_else(|_| "user".into());
            PathBuf::from(format!("/tmp/lamppost-{name}-{user}.sock"))
        }
    }
}

/// Send one line to a running part of LAMPPost. False if nobody listens.
fn tell(name: &str, message: &str) -> bool {
    match UnixStream::connect(socket_path(name)) {
        Ok(mut stream) => stream.write_all(format!("{message}\n").as_bytes()).is_ok(),
        Err(_) => false,
    }
}

fn is_running(name: &str) -> bool {
    UnixStream::connect(socket_path(name)).is_ok()
}

/// Listen for one-line commands and hand them to `on_line`.
fn serve(name: &str, on_line: impl Fn(&str) + Send + 'static) {
    let path = socket_path(name);
    let _ = std::fs::remove_file(&path);
    let Ok(listener) = UnixListener::bind(&path) else { return };
    std::thread::Builder::new()
        .name(format!("{name}-socket"))
        .spawn(move || {
            for stream in listener.incoming().flatten() {
                for line in BufReader::new(stream).lines().map_while(Result::ok) {
                    on_line(line.trim());
                }
            }
        })
        .ok();
}

fn start_window() {
    cli::detached(&settings::own_program().to_string_lossy(), &[]);
}

/// A module could not start because it is not installed: let the window
/// offer the installation (the tray has no way to ask for confirmation).
fn offer_install(keys: &[String]) {
    if !tell("window", &format!("install {}", keys.join(" "))) {
        let mut args = vec!["--install"];
        args.extend(keys.iter().map(String::as_str));
        cli::detached(&settings::own_program().to_string_lossy(), &args);
    }
}

/// Start/stop from the tray; exit code 3 means "not installed".
fn run_from_tray(action: &'static str, keys: Vec<String>) {
    cli::run(action, keys, |done| {
        if done.code == 3 {
            offer_install(&done.keys);
        } else if done.code != 0 {
            eprintln!("lamppost: {} failed: {}", done.action, done.output.trim());
        }
    });
}

fn start_tray() {
    cli::detached(&settings::own_program().to_string_lossy(), &["--tray"]);
}

// ------------------------------------------------------------------ tray mode
/// The tray icon without a window: a few megabytes, no graphics stack.
fn tray_mode() -> eframe::Result<()> {
    if is_running("tray") {
        return Ok(()); // one tray icon is enough
    }
    let (tx, rx) = crossbeam_channel::unbounded::<tray::Cmd>();

    let socket_tx = tx.clone();
    serve("tray", move |line| {
        let cmd = match line {
            "show" => tray::Cmd::Show,
            "quit" => tray::Cmd::Quit,
            _ => return,
        };
        let _ = socket_tx.send(cmd);
    });

    let state_tx = tray::spawn(tx);
    let mut modules = modules::all();
    let mut shown: Option<tray::State> = None;

    loop {
        for module in &mut modules {
            module.refresh();
        }
        // only talk to the desktop when something actually changed
        let state = tray::State {
            modules: modules
                .iter()
                .map(|m| (m.key.to_string(), m.name.to_string(), m.running(), m.busy))
                .collect(),
        };
        if shown.as_ref() != Some(&state) {
            let _ = state_tx.send(state.clone());
            shown = Some(state);
        }

        // wait for the next command, but look at the modules again regularly
        match rx.recv_timeout(Duration::from_millis(1500)) {
            Ok(tray::Cmd::Show) => {
                if !tell("window", "show") {
                    start_window();
                }
            }
            Ok(tray::Cmd::Toggle(key)) => {
                let running = modules.iter().any(|m| m.key == key && m.running());
                run_from_tray(if running { "stop" } else { "start" }, vec![key]);
            }
            Ok(tray::Cmd::StartWeb) => {
                let keys = ["apache", "mariadb"]
                    .iter()
                    .filter(|k| modules.iter().any(|m| &m.key == *k && !m.running()))
                    .map(|k| k.to_string())
                    .collect::<Vec<_>>();
                if !keys.is_empty() {
                    run_from_tray("start", keys);
                }
            }
            Ok(tray::Cmd::StopAll) => {
                let keys: Vec<String> =
                    modules.iter().filter(|m| m.running()).map(|m| m.key.to_string()).collect();
                if !keys.is_empty() {
                    run_from_tray("stop", keys);
                }
            }
            Ok(tray::Cmd::OpenUrl(url)) => {
                cli::open_url(&url);
            }
            Ok(tray::Cmd::Quit) => {
                tell("window", "quit");
                return Ok(());
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => return Ok(()),
        }
    }
}

// ---------------------------------------------------------------- window mode
fn window_mode(install: Vec<String>) -> eframe::Result<()> {
    // a second start just brings the open window forward
    let hello = if install.is_empty() { "show".to_string() } else { format!("install {}", install.join(" ")) };
    if tell("window", &hello) {
        return Ok(());
    }

    let (tx, rx) = crossbeam_channel::unbounded::<Msg>();
    let socket_tx = tx.clone();
    serve("window", move |line| {
        let msg = match line.split_once(' ') {
            Some(("install", keys)) => {
                Msg::OfferInstall(keys.split_whitespace().filter(|k| is_module(k)).map(str::to_string).collect())
            }
            _ => match line {
                "show" => Msg::ShowWindow,
                "quit" => Msg::Quit,
                _ => return,
            },
        };
        let _ = socket_tx.send(msg);
        wake_ui();
    });
    if !install.is_empty() {
        let _ = tx.send(Msg::OfferInstall(install));
    }

    // the tray belongs to its own process, so closing the window can really
    // close it without LAMPPost disappearing from the top bar
    if !is_running("tray") {
        start_tray();
    }

    let size = settings::Settings::load().window_size().unwrap_or([940.0, 660.0]);
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("LAMPPost Control Panel")
            .with_app_id("lamppost-control")
            .with_inner_size(size)
            .with_min_inner_size([700.0, 480.0])
            // the window frame is drawn by the panel itself, so the title bar
            // and its buttons follow the same theme as everything else
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(true),
        ..Default::default()
    };

    eframe::run_native(
        "lamppost-control",
        options,
        Box::new(move |cc| {
            let _ = CTX.set(cc.egui_ctx.clone());
            Ok(Box::new(app::App::new(&cc.egui_ctx, rx, tx)))
        }),
    )
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print!("{HELP}");
        return Ok(());
    }
    if args.iter().any(|a| a == "--tray") {
        return tray_mode();
    }
    // --install <module...>: open the window with the installation offer
    let install: Vec<String> = match args.iter().position(|a| a == "--install") {
        Some(at) => args[at + 1..].iter().filter(|k| is_module(k)).cloned().collect(),
        None => Vec::new(),
    };
    window_mode(install)
}

fn is_module(key: &str) -> bool {
    settings::MODULE_KEYS.contains(&key)
}

/// Used by the window when it closes: make sure the tray is still there.
pub fn ensure_tray() {
    if !is_running("tray") {
        start_tray();
    }
}

/// Used by the window's Quit button: the tray goes too.
pub fn quit_tray() {
    tell("tray", "quit");
}

