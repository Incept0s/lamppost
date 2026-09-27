//! The tray icon (StatusNotifierItem), shown by GNOME's AppIndicator
//! extension and natively by KDE and others.
use crossbeam_channel::Sender;
use ksni::TrayMethods;

/// What the tray icon asks for. Handled by the tray process itself.
pub enum Cmd {
    Show,
    Toggle(String),
    StartWeb,
    StopAll,
    OpenUrl(String),
    Quit,
}

/// What the tray shows: which modules run, so its menu can offer the opposite.
#[derive(Clone, Default, PartialEq)]
pub struct State {
    pub modules: Vec<(String, String, bool, bool)>, // key, name, running, busy
}

impl State {
    fn running_names(&self) -> Vec<&str> {
        self.modules.iter().filter(|(_, _, r, _)| *r).map(|(_, n, _, _)| n.as_str()).collect()
    }
}

pub struct Tray {
    state: State,
    tx: Sender<Cmd>,
}

fn pixmap(bytes: &[u8]) -> ksni::Icon {
    // ksni wants ARGB32 in network byte order; the file is plain RGBA
    let mut data = Vec::with_capacity(bytes.len());
    for chunk in bytes.as_chunks::<4>().0 {
        data.extend_from_slice(&[chunk[3], chunk[0], chunk[1], chunk[2]]);
    }
    ksni::Icon { width: 64, height: 64, data }
}

impl ksni::Tray for Tray {
    fn id(&self) -> String {
        "lamppost".into()
    }

    fn title(&self) -> String {
        "LAMPPost".into()
    }

    fn icon_name(&self) -> String {
        "lamppost".into()
    }

    fn icon_pixmap(&self) -> Vec<ksni::Icon> {
        // used when the icon is not in the icon theme, e.g. from an AppImage;
        // converted once, the host asks for it on every update
        static ON: std::sync::OnceLock<ksni::Icon> = std::sync::OnceLock::new();
        static OFF: std::sync::OnceLock<ksni::Icon> = std::sync::OnceLock::new();
        let icon = if self.state.running_names().is_empty() {
            OFF.get_or_init(|| pixmap(include_bytes!("../../assets/tray-64-off.rgba")))
        } else {
            ON.get_or_init(|| pixmap(include_bytes!("../../assets/tray-64.rgba")))
        };
        vec![icon.clone()]
    }

    fn tool_tip(&self) -> ksni::ToolTip {
        let running = self.state.running_names();
        ksni::ToolTip {
            title: "LAMPPost".into(),
            description: if running.is_empty() {
                "All modules stopped".into()
            } else {
                format!("Running: {}", running.join(", "))
            },
            ..Default::default()
        }
    }

    fn activate(&mut self, _x: i32, _y: i32) {
        let _ = self.tx.send(Cmd::Show);
    }

    fn menu(&self) -> Vec<ksni::MenuItem<Self>> {
        use ksni::menu::*;
        let mut items: Vec<ksni::MenuItem<Self>> = vec![
            StandardItem {
                label: "Open Control Panel".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send(Cmd::Show);
                }),
                ..Default::default()
            }
            .into(),
            MenuItem::Separator,
        ];
        for (key, name, running, busy) in &self.state.modules {
            let key = key.clone();
            items.push(
                StandardItem {
                    label: format!("{} {}", if *running { "Stop" } else { "Start" }, name),
                    enabled: !*busy,
                    activate: Box::new(move |t: &mut Self| {
                        let _ = t.tx.send(Cmd::Toggle(key.clone()));
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Start Apache + MariaDB".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send(Cmd::StartWeb);
                }),
                ..Default::default()
            }
            .into(),
        );
        items.push(
            StandardItem {
                label: "Stop all".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send(Cmd::StopAll);
                }),
                ..Default::default()
            }
            .into(),
        );
        items.push(MenuItem::Separator);
        for (label, url) in [
            ("Open dashboard", "http://localhost/"),
            ("Open phpMyAdmin", "http://localhost/phpmyadmin/"),
        ] {
            items.push(
                StandardItem {
                    label: label.into(),
                    activate: Box::new(move |t: &mut Self| {
                        let _ = t.tx.send(Cmd::OpenUrl(url.to_string()));
                    }),
                    ..Default::default()
                }
                .into(),
            );
        }
        items.push(MenuItem::Separator);
        items.push(
            StandardItem {
                label: "Quit".into(),
                activate: Box::new(|t: &mut Self| {
                    let _ = t.tx.send(Cmd::Quit);
                }),
                ..Default::default()
            }
            .into(),
        );
        items
    }
}

/// Start the tray on its own thread; returns the channel to update its state.
pub fn spawn(tx: Sender<Cmd>) -> tokio::sync::mpsc::UnboundedSender<State> {
    let (state_tx, mut state_rx) = tokio::sync::mpsc::unbounded_channel::<State>();
    let failed = tx.clone();
    std::thread::Builder::new()
        .name("tray".into())
        .spawn(move || {
            let runtime = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
                Ok(runtime) => runtime,
                Err(e) => return eprintln!("lamppost: no tray ({e})"),
            };
            runtime.block_on(async move {
                let handle = match (Tray { state: State::default(), tx }).spawn().await {
                    Ok(handle) => handle,
                    Err(e) => {
                        eprintln!("lamppost: tray icon unavailable ({e}) - is the AppIndicator extension enabled?");
                        // without an icon there is nothing to click, so do not
                        // stay behind as a process nobody can reach
                        let _ = failed.send(Cmd::Quit);
                        return;
                    }
                };
                while let Some(state) = state_rx.recv().await {
                    handle.update(|tray| tray.state = state.clone()).await;
                }
            });
        })
        .ok();
    state_tx
}
