//! Small questions about the system: who listens on which port, which system
//! services are active, which group we are in.
use std::collections::HashMap;
use std::process::Command;

pub struct Socket {
    pub address: String,
    pub port: u16,
    /// empty when the process belongs to another user
    pub pid: String,
    pub name: String,
    pub scope: Scope,
}

/// Who can reach a listening socket.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// loopback only: this computer
    Local,
    /// 0.0.0.0, :: or *: every interface, so also the network
    All,
    /// one specific non-loopback address
    Network,
}

impl Scope {
    fn of(address: &str) -> Scope {
        if matches!(address, "0.0.0.0" | "::" | "*") {
            Scope::All
        } else if address.starts_with("127.") || address == "::1" {
            Scope::Local
        } else {
            Scope::Network
        }
    }
}

/// "[::ffff:127.0.0.1]" -> "127.0.0.1", "127.0.0.53%lo" -> "127.0.0.53", "[::1]" -> "::1"
fn clean_address(raw: &str) -> String {
    let unbracketed = raw.trim_start_matches('[').trim_end_matches(']');
    let without_zone = unbracketed.split('%').next().unwrap_or(unbracketed);
    without_zone.strip_prefix("::ffff:").unwrap_or(without_zone).to_string()
}

/// The full program name: ss only knows the kernel's 15-character one.
fn program_name(pid: &str, fallback: &str) -> String {
    std::fs::read(format!("/proc/{pid}/cmdline"))
        .ok()
        .and_then(|raw| {
            let first = raw.split(|b| *b == 0).next()?.to_vec();
            let first = String::from_utf8(first).ok()?;
            let base = first.rsplit('/').next()?.to_string();
            (!base.is_empty()).then_some(base)
        })
        .unwrap_or_else(|| fallback.to_string())
}

/// Every listening TCP socket, via "ss", sorted by port.
pub fn listening() -> Vec<Socket> {
    let Ok(out) = Command::new("ss").args(["-Htlnp"]).output() else { return Vec::new() };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut sockets = Vec::new();
    for line in text.lines() {
        let columns: Vec<&str> = line.split_whitespace().collect();
        if columns.len() < 4 {
            continue;
        }
        let (address, port) = columns[3].rsplit_once(':').unwrap_or(("", ""));
        let Ok(port) = port.parse::<u16>() else { continue };
        let (mut name, mut pid) = (String::new(), String::new());
        if let Some(rest) = line.split_once("users:((").map(|(_, r)| r) {
            name = rest.split('"').nth(1).unwrap_or("").to_string();
            pid = rest
                .split_once("pid=")
                .map(|(_, r)| r.split(',').next().unwrap_or("").to_string())
                .unwrap_or_default();
        }
        if !pid.is_empty() {
            name = program_name(&pid, &name);
        }
        let address = clean_address(address);
        let scope = Scope::of(&address);
        sockets.push(Socket { address, port, pid, name, scope });
    }
    sockets.sort_by(|a, b| a.port.cmp(&b.port).then_with(|| a.address.cmp(&b.address)));
    sockets
}

/// {port: "process (PID 123)"} for the ports that are in use.
pub fn busy_ports() -> HashMap<u16, String> {
    let mut busy = HashMap::new();
    for socket in listening() {
        let owner = if socket.name.is_empty() {
            String::new()
        } else {
            format!("{} (PID {})", socket.name, socket.pid)
        };
        busy.entry(socket.port).or_insert(owner);
    }
    busy
}

pub fn systemctl(verb: &str, unit: &str) -> String {
    Command::new("systemctl")
        .args([verb, unit])
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".into())
}

pub fn os_pretty_name() -> String {
    std::fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| {
            text.lines()
                .find_map(|l| l.strip_prefix("PRETTY_NAME=").map(|v| v.trim_matches('"').to_string()))
        })
        .unwrap_or_else(|| "Linux".into())
}

pub fn machine() -> String {
    Command::new("uname")
        .arg("-m")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

/// The group that may read phpMyAdmin's configuration on this distribution.
pub fn web_group() -> Option<&'static str> {
    let groups = std::fs::read_to_string("/etc/group").unwrap_or_default();
    ["apache", "www-data"]
        .into_iter()
        .find(|name| groups.lines().any(|line| line.starts_with(&format!("{name}:"))))
}

/// Is the user in this group in the current session?
pub fn in_group(group: &str) -> bool {
    Command::new("id")
        .arg("-nG")
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).split_whitespace().any(|g| g == group))
        .unwrap_or(true)
}

/// Lowest port a normal program may use (LAMPPost needs 21).
pub fn unprivileged_port_start() -> u32 {
    std::fs::read_to_string("/proc/sys/net/ipv4/ip_unprivileged_port_start")
        .ok()
        .and_then(|v| v.trim().parse().ok())
        .unwrap_or(1024)
}
