//! The five modules and how to see whether they run.
//!
//! Starting and stopping lives in the "lamppost" shell script; the panel only
//! watches PID files and ports, so anything it does you can do in a terminal.
use crate::settings::path;
use std::path::PathBuf;

pub struct Entry {
    pub label: &'static str,
    /// file to open, or a directory to browse; may contain a "*" glob
    pub target: PathBuf,
}

pub enum Admin {
    Url(&'static str),
    Dir(PathBuf),
}

pub struct Module {
    pub key: &'static str,
    pub name: &'static str,
    pub ports: &'static [u16],
    pub pidfile: PathBuf,
    /// text that must appear in /proc/<pid>/cmdline: our own config path, so
    /// a stale PID file or a recycled PID is never mistaken for the module
    pub pattern: String,
    pub engine: &'static str,
    pub admin: Admin,
    pub configs: Vec<Entry>,
    pub logs: Vec<Entry>,
    // live state, refreshed by the status timer
    pub pids: Vec<i32>,
    pub busy: bool,
    pub version: String,
}

fn entry(label: &'static str, target: PathBuf) -> Entry {
    Entry { label, target }
}

pub fn all() -> Vec<Module> {
    vec![
        Module {
            key: "apache",
            name: "Apache",
            ports: &[80, 443],
            pidfile: path("apache/logs/httpd.pid"),
            pattern: path("apache/conf/httpd.conf").display().to_string(),
            engine: "Apache HTTP Server + PHP-FPM",
            admin: Admin::Url("http://localhost/"),
            configs: vec![
                entry("Apache (httpd.conf)", path("apache/conf/httpd.conf")),
                entry("Apache (httpd-ssl.conf)", path("apache/conf/extra/httpd-ssl.conf")),
                entry("Apache (httpd-vhosts.conf)", path("apache/conf/extra/httpd-vhosts.conf")),
                entry("PHP (php.ini)", path("php/php.ini")),
                entry("PHP-FPM (php-fpm.conf)", path("php/php-fpm.conf")),
                entry("phpMyAdmin (config.inc.php)", path("phpMyAdmin/config.inc.php")),
                entry("Browse: Apache", path("apache")),
                entry("Browse: PHP", path("php")),
                entry("Browse: htdocs", path("htdocs")),
            ],
            logs: vec![
                entry("Apache (access_log)", path("apache/logs/access_log")),
                entry("Apache (error_log)", path("apache/logs/error_log")),
                entry("Apache (ssl_error_log)", path("apache/logs/ssl_error_log")),
                entry("PHP (php_error_log)", path("php/logs/php_error_log")),
                entry("PHP-FPM (php-fpm.log)", path("php/logs/php-fpm.log")),
            ],
            pids: Vec::new(),
            busy: false,
            version: String::new(),
        },
        Module {
            key: "mariadb",
            name: "MariaDB",
            ports: &[3306],
            pidfile: path("mariadb/data/mariadb.pid"),
            pattern: path("mariadb/my.cnf").display().to_string(),
            engine: "MariaDB database server",
            admin: Admin::Url("http://localhost/phpmyadmin/"),
            configs: vec![
                entry("my.cnf", path("mariadb/my.cnf")),
                entry("Browse: MariaDB", path("mariadb")),
            ],
            logs: vec![entry("mariadb_error.log", path("mariadb/data/mariadb_error.log"))],
            pids: Vec::new(),
            busy: false,
            version: String::new(),
        },
        Module {
            key: "ftp",
            name: "FTP Server",
            ports: &[21],
            pidfile: path("ftp/ftpd.pid"),
            pattern: path("ftp/lamppost-ftpd").display().to_string(),
            engine: "pyftpdlib - users and passwords in lamppost-ftp.ini",
            admin: Admin::Url("ftp://localhost/"),
            configs: vec![
                entry("lamppost-ftp.ini (users & passwords)", path("ftp/lamppost-ftp.ini")),
                entry("Browse: ftp", path("ftp")),
            ],
            logs: vec![
                entry("ftpd.log", path("ftp/logs/ftpd.log")),
                entry("ftpd.out", path("ftp/logs/ftpd.out")),
            ],
            pids: Vec::new(),
            busy: false,
            version: String::new(),
        },
        Module {
            key: "mail",
            name: "Mail Catcher",
            ports: &[25],
            pidfile: path("mail/mail.pid"),
            pattern: path("mail/mail-catcher").display().to_string(),
            engine: "aiosmtpd - saves mail to mailoutput instead of sending it",
            admin: Admin::Dir(path("mailoutput")),
            configs: vec![
                entry("Browse: mail", path("mail")),
                entry("Browse: mailoutput", path("mailoutput")),
                entry("Browse: mailtodisk", path("mailtodisk")),
            ],
            logs: vec![entry("mail.log", path("mail/mail.log"))],
            pids: Vec::new(),
            busy: false,
            version: String::new(),
        },
        Module {
            key: "tomcat",
            name: "Tomcat",
            ports: &[8080, 8005],
            pidfile: path("tomcat/temp/tomcat.pid"),
            pattern: format!("catalina.base={}", path("tomcat").display()),
            engine: "Apache Tomcat",
            admin: Admin::Url("http://localhost:8080/"),
            configs: vec![
                entry("server.xml", path("tomcat/conf/server.xml")),
                entry("tomcat-users.xml", path("tomcat/conf/tomcat-users.xml")),
                entry("web.xml", path("tomcat/conf/web.xml")),
                entry("context.xml", path("tomcat/conf/context.xml")),
                entry("Browse: tomcat", path("tomcat")),
            ],
            logs: vec![
                entry("catalina.out", path("tomcat/logs/catalina.out")),
                entry("catalina.<date>.log", path("tomcat/logs/catalina.*.log")),
                entry("localhost_access_log", path("tomcat/logs/localhost_access_log.*.txt")),
            ],
            pids: Vec::new(),
            busy: false,
            version: String::new(),
        },
    ]
}

impl Module {
    pub fn running(&self) -> bool {
        !self.pids.is_empty()
    }

    /// Version line under the name; the engine is in the tooltip.
    pub fn info(&self) -> &str {
        if self.version.is_empty() { self.engine } else { &self.version }
    }

    /// Look at the PID file and (for Apache) its worker processes.
    pub fn refresh(&mut self) {
        self.pids.clear();
        let Some(pid) = read_pid(&self.pidfile) else { return };
        if !cmdline_contains(pid, &self.pattern) {
            return;
        }
        self.pids.push(pid);
        if self.key == "apache" {
            self.pids.extend(child_pids(pid));
        }
    }
}

pub fn read_pid(file: &std::path::Path) -> Option<i32> {
    let text = std::fs::read_to_string(file).ok()?;
    let digits: String = text.lines().next()?.chars().filter(char::is_ascii_digit).collect();
    digits.parse().ok()
}

pub fn cmdline_contains(pid: i32, pattern: &str) -> bool {
    let Ok(raw) = std::fs::read(format!("/proc/{pid}/cmdline")) else { return false };
    String::from_utf8_lossy(&raw).replace('\0', " ").contains(pattern)
}

/// Direct children of a process, from /proc/<pid>/task/*/children (cheap).
pub fn child_pids(parent: i32) -> Vec<i32> {
    let mut kids = Vec::new();
    let Ok(tasks) = std::fs::read_dir(format!("/proc/{parent}/task")) else { return kids };
    for task in tasks.flatten() {
        if let Ok(list) = std::fs::read_to_string(task.path().join("children")) {
            kids.extend(list.split_whitespace().filter_map(|p| p.parse::<i32>().ok()));
        }
    }
    kids.sort_unstable();
    kids
}
