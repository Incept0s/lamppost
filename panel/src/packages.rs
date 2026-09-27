//! Versions and updates.
//!
//! LAMPPost runs the distribution's packages, so "updating Apache" means
//! updating the httpd (or apache2) package. Checking works as your user;
//! installing goes through pkexec and asks for your password once.
use std::process::Command;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Distro {
    /// Fedora, RHEL, openSUSE: rpm + dnf
    Rpm,
    /// Debian, Ubuntu, Mint: dpkg + apt
    Deb,
    Unknown,
}

/// Detected once: it scans PATH, and the updates window asks every frame.
pub fn distro() -> Distro {
    static DISTRO: std::sync::OnceLock<Distro> = std::sync::OnceLock::new();
    *DISTRO.get_or_init(|| {
        if crate::settings::which("rpm").is_some() {
            Distro::Rpm
        } else if crate::settings::which("dpkg-query").is_some() {
            Distro::Deb
        } else {
            Distro::Unknown
        }
    })
}

fn package_manager() -> &'static str {
    if crate::settings::which("dnf5").is_some() { "dnf5" } else { "dnf" }
}

/// (title, module key, main package, all packages)
pub struct Component {
    pub title: &'static str,
    pub key: Option<&'static str>,
    pub main: &'static str,
    pub packages: &'static [&'static str],
}

const RPM_COMPONENTS: &[Component] = &[
    Component { title: "Apache HTTP Server", key: Some("apache"), main: "httpd",
                packages: &["httpd", "mod_ssl"] },
    Component { title: "PHP", key: Some("apache"), main: "php-fpm",
                packages: &["php-fpm", "php-cli", "php-mysqlnd", "php-gd", "php-mbstring",
                            "php-intl", "php-xml", "php-pecl-zip", "php-sodium"] },
    Component { title: "phpMyAdmin", key: Some("apache"), main: "phpMyAdmin",
                packages: &["phpMyAdmin"] },
    Component { title: "MariaDB", key: Some("mariadb"), main: "mariadb-server",
                packages: &["mariadb-server", "mariadb"] },
    Component { title: "FTP server (pyftpdlib)", key: Some("ftp"), main: "python3-pyftpdlib",
                packages: &["python3-pyftpdlib"] },
    Component { title: "Mail catcher (aiosmtpd)", key: Some("mail"), main: "python3-aiosmtpd",
                packages: &["python3-aiosmtpd"] },
    Component { title: "Tomcat", key: Some("tomcat"), main: "tomcat",
                packages: &["tomcat", "tomcat-webapps", "tomcat-admin-webapps"] },
];

const DEB_COMPONENTS: &[Component] = &[
    Component { title: "Apache HTTP Server", key: Some("apache"), main: "apache2",
                packages: &["apache2", "apache2-utils"] },
    Component { title: "PHP", key: Some("apache"), main: "php-fpm",
                packages: &["php-fpm", "php-cli", "php-mysql", "php-gd", "php-mbstring",
                            "php-intl", "php-xml", "php-zip"] },
    Component { title: "phpMyAdmin", key: Some("apache"), main: "phpmyadmin",
                packages: &["phpmyadmin"] },
    Component { title: "MariaDB", key: Some("mariadb"), main: "mariadb-server",
                packages: &["mariadb-server", "mariadb-client"] },
    Component { title: "FTP server (pyftpdlib)", key: Some("ftp"), main: "python3-pyftpdlib",
                packages: &["python3-pyftpdlib"] },
    Component { title: "Mail catcher (aiosmtpd)", key: Some("mail"), main: "python3-aiosmtpd",
                packages: &["python3-aiosmtpd"] },
    Component { title: "Tomcat", key: Some("tomcat"), main: "tomcat10",
                packages: &["tomcat10", "tomcat10-admin"] },
];

pub fn components() -> &'static [Component] {
    match distro() {
        Distro::Deb => DEB_COMPONENTS,
        _ => RPM_COMPONENTS,
    }
}

pub fn all_packages() -> Vec<&'static str> {
    let mut names: Vec<&'static str> = components().iter().flat_map(|c| c.packages.iter().copied()).collect();
    names.sort_unstable();
    names.dedup();
    names
}

/// "2.4.68-1.fc44" -> "2.4.68", "1:11.8.8-1" -> "11.8.8"
pub fn short_version(version: &str) -> String {
    let without_epoch = version.split_once(':').map_or(version, |(_, v)| v);
    without_epoch.split('-').next().unwrap_or(without_epoch).to_string()
}

fn parse_pairs(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|line| {
            let mut parts = line.split_whitespace();
            Some((parts.next()?.to_string(), parts.next()?.to_string()))
        })
        .filter(|(name, _)| !name.starts_with("package"))
        .collect()
}

/// Installed version of every package LAMPPost cares about.
pub fn installed() -> Vec<(String, String)> {
    let packages = all_packages();
    let output = match distro() {
        Distro::Deb => Command::new("dpkg-query")
            .args(["-W", "-f", "${Package} ${Version}\n"])
            .args(&packages)
            .output(),
        _ => Command::new("rpm")
            .args(["-q", "--qf", "%{name} %{version}-%{release}\n"])
            .args(&packages)
            .output(),
    };
    output
        .map(|o| parse_pairs(&String::from_utf8_lossy(&o.stdout)))
        .unwrap_or_default()
}

/// "Apache 2.4.68 · PHP 8.5.10" per module, for the line under its name.
pub fn module_versions() -> Vec<(String, String)> {
    let have = installed();
    let short = |name: &str| {
        have.iter().find(|(p, _)| p == name).map(|(_, v)| short_version(v))
    };
    let mut labels: Vec<(String, String)> = Vec::new();
    for component in components() {
        let (Some(key), Some(version)) = (component.key, short(component.main)) else { continue };
        let name = match component.title {
            "Apache HTTP Server" => "Apache",
            "FTP server (pyftpdlib)" => "pyftpdlib",
            "Mail catcher (aiosmtpd)" => "aiosmtpd",
            other => other,
        };
        let text = format!("{name} {version}");
        match labels.iter_mut().find(|(k, _)| k == key) {
            Some((_, existing)) => {
                existing.push_str(" · ");
                existing.push_str(&text);
            }
            None => labels.push((key.to_string(), text)),
        }
    }
    labels
}

/// The command that lists packages with a newer version available.
pub fn check_command(refresh: bool) -> (String, Vec<String>) {
    let packages = all_packages();
    match distro() {
        Distro::Deb => {
            // apt has no "only these packages" query, so we filter afterwards
            let _ = (refresh, &packages);
            ("apt".to_string(), vec!["list".into(), "--upgradable".into()])
        }
        _ => {
            let mut args: Vec<String> = vec![
                "-q".into(), "repoquery".into(), "--upgrades".into(),
                "--latest-limit".into(), "1".into(),
                "--qf".into(), "%{name} %{evr}\n".into(),
            ];
            if refresh {
                args.push("--refresh".into());
            }
            args.extend(packages.iter().map(|p| p.to_string()));
            (package_manager().to_string(), args)
        }
    }
}

/// {package: new version} from the output of `check_command`.
pub fn parse_upgrades(output: &str) -> Vec<(String, String)> {
    let wanted = all_packages();
    match distro() {
        Distro::Deb => output
            .lines()
            .filter_map(|line| {
                // "apache2/stable 2.4.62-1 amd64 [upgradable from: 2.4.61-1]"
                let (name, rest) = line.split_once('/')?;
                let version = rest.split_whitespace().nth(1)?;
                wanted.contains(&name).then(|| (name.to_string(), version.to_string()))
            })
            .collect(),
        _ => parse_pairs(output)
            .into_iter()
            .filter(|(name, _)| wanted.contains(&name.as_str()))
            .collect(),
    }
}

/// The command that installs the updates - this one needs the password.
pub fn install_command(packages: &[String]) -> (String, Vec<String>) {
    match distro() {
        Distro::Deb => {
            let mut args = vec!["apt-get".to_string(), "install".into(), "-y".into(),
                                "--only-upgrade".into()];
            args.extend(packages.iter().cloned());
            ("pkexec".to_string(), args)
        }
        _ => {
            let mut args = vec![package_manager().to_string(), "-y".into(), "upgrade".into()];
            args.extend(packages.iter().cloned());
            ("pkexec".to_string(), args)
        }
    }
}

/// Programs that must exist for the modules to start. Distributions put them
/// in different places, so each entry lists the candidates.
pub fn required_programs() -> Vec<(&'static str, Vec<&'static str>)> {
    vec![
        ("Apache", vec!["/usr/bin/httpd", "/usr/sbin/httpd", "/usr/sbin/apache2"]),
        ("PHP-FPM", vec!["/usr/bin/php-fpm", "/usr/sbin/php-fpm"]),
        ("MariaDB", vec!["/usr/bin/mariadbd", "/usr/sbin/mariadbd", "/usr/sbin/mysqld"]),
        ("Java (for Tomcat)", vec!["/usr/bin/java"]),
    ]
}

/// Debian names the PHP-FPM binary after the version (php-fpm8.3).
pub fn have_program(candidates: &[&str]) -> bool {
    if candidates.iter().any(|c| std::path::Path::new(c).exists()) {
        return true;
    }
    candidates.iter().any(|c| {
        let Some((dir, prefix)) = c.rsplit_once('/') else { return false };
        std::fs::read_dir(dir)
            .map(|entries| {
                entries.flatten().any(|e| e.file_name().to_string_lossy().starts_with(prefix))
            })
            .unwrap_or(false)
    })
}
