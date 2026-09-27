<?php
// LAMPPost dashboard - shows what is running and which versions are installed.

function port_open(int $port): bool {
    $fp = @fsockopen('127.0.0.1', $port, $errno, $errstr, 0.3);
    if ($fp) { fclose($fp); return true; }
    return false;
}

function mariadb_version(): string {
    if (!function_exists('mysqli_connect')) return 'mysqli extension missing';
    mysqli_report(MYSQLI_REPORT_OFF);
    $db = @mysqli_connect('localhost', 'root', '');
    if (!$db) return 'not running';
    $v = mysqli_get_server_info($db);
    mysqli_close($db);
    return $v;
}

function phpmyadmin_version(): string {
    $file = '/usr/share/phpMyAdmin/libraries/classes/Version.php';
    $src = @file_get_contents($file);
    if ($src && preg_match("/VERSION\s*=\s*'([^']+)'/", $src, $m)) return $m[1];
    return 'installed';
}

$apache = $_SERVER['SERVER_SOFTWARE'] ?? 'Apache';
$rows = [
    ['Apache',     $apache,                 true],
    ['PHP',        PHP_VERSION . ' (' . PHP_SAPI . ')', true],
    ['MariaDB',    mariadb_version(),       port_open(3306)],
    ['phpMyAdmin', phpmyadmin_version(),    true],
    ['FTP Server',   'FTP on port 21', port_open(21)],
    ['Mail Catcher', 'SMTP on port 25', port_open(25)],
    ['Tomcat',     'Apache Tomcat on port 8080', port_open(8080)],
];
$h = fn($s) => htmlspecialchars((string)$s, ENT_QUOTES);
?><!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>LAMPPost Dashboard</title>
<style>
  :root { --orange: #fb7a24; --dark: #2f2f2f; }
  * { box-sizing: border-box; }
  body { margin: 0; font-family: system-ui, "Cantarell", "Segoe UI", sans-serif; color: var(--dark); background: #f4f4f4; }
  header { background: var(--dark); color: #fff; }
  .bar { max-width: 960px; margin: 0 auto; padding: 14px 20px; display: flex; align-items: center; gap: 14px; flex-wrap: wrap; }
  .bar img { width: 40px; height: 40px; }
  .bar b { font-size: 1.4rem; letter-spacing: .02em; }
  nav { margin-left: auto; display: flex; gap: 18px; flex-wrap: wrap; }
  nav a { color: #fff; text-decoration: none; opacity: .85; }
  nav a:hover { opacity: 1; color: var(--orange); }
  .hero { background: var(--orange); color: #fff; }
  .hero .inner { max-width: 960px; margin: 0 auto; padding: 42px 20px; }
  .hero h1 { margin: 0 0 8px; font-size: 2.2rem; font-weight: 600; }
  .hero p { margin: 0; font-size: 1.1rem; opacity: .95; }
  main { max-width: 960px; margin: 0 auto; padding: 28px 20px 40px; }
  .card { background: #fff; border-radius: 6px; padding: 20px 22px; margin-bottom: 20px; box-shadow: 0 1px 3px rgba(0,0,0,.08); }
  h2 { margin-top: 0; font-size: 1.25rem; }
  table { width: 100%; border-collapse: collapse; }
  td { padding: 8px 6px; border-bottom: 1px solid #eee; vertical-align: top; }
  td:first-child { font-weight: 600; width: 150px; white-space: nowrap; }
  .dot { display: inline-block; width: 10px; height: 10px; border-radius: 50%; margin-right: 6px; }
  .on { background: #2eaa4a; } .off { background: #bbb; }
  code { background: #f1f1f1; padding: 1px 5px; border-radius: 3px; }
  footer { text-align: center; color: #777; font-size: .85rem; padding: 0 20px 30px; }
</style>
</head>
<body>
<header>
  <div class="bar">
    <img src="lamppost.svg" alt="">
    <b>LAMPPost</b> <span>Apache + MariaDB + PHP + phpMyAdmin</span>
    <nav>
      <a href="/dashboard/phpinfo.php">PHPInfo</a>
      <a href="/phpmyadmin/">phpMyAdmin</a>
      <a href="http://localhost:8080/">Tomcat</a>
      <a href="/server-status">Server status</a>
    </nav>
  </div>
</header>
<section class="hero">
  <div class="inner">
    <h1>LAMPPost local development server</h1>
    <p>Document root: <b>/opt/lamppost/htdocs</b>. Projects placed there are served at <b>http://localhost/&lt;project&gt;</b>.</p>
  </div>
</section>
<main>
  <div class="card">
    <h2>Modules</h2>
    <table>
      <?php foreach ($rows as [$name, $info, $up]): ?>
      <tr><td><span class="dot <?= $up ? 'on' : 'off' ?>"></span><?= $h($name) ?></td><td><?= $h($info) ?></td></tr>
      <?php endforeach; ?>
    </table>
  </div>
  <div class="card">
    <h2>Where is what?</h2>
    <table>
      <tr><td>Websites</td><td><code>/opt/lamppost/htdocs</code></td></tr>
      <tr><td>Databases</td><td>phpMyAdmin (user <code>root</code>, no password) or <code>/opt/lamppost/mariadb/bin/mysql</code></td></tr>
      <tr><td>Sent mail</td><td><code>/opt/lamppost/mailoutput</code> - mail() and SMTP to localhost:25 are saved here, never sent</td></tr>
      <tr><td>FTP</td><td><code>ftp://localhost</code> - user and password in <code>/opt/lamppost/ftp/lamppost-ftp.ini</code> (lands in htdocs)</td></tr>
      <tr><td>Settings</td><td>Control Panel &rarr; Config, or the files in <code>/opt/lamppost</code></td></tr>
    </table>
  </div>
</main>
<footer>LAMPPost &mdash; an XAMPP-style local development stack. Not affiliated with Apache Friends or the XAMPP project. All servers are your distribution's packages and are updated by its package manager.</footer>
</body>
</html>
