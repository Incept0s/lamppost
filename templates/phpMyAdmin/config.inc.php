<?php
/**
 * LAMPPost - phpMyAdmin settings
 *
 * phpMyAdmin itself is Fedora's package (/usr/share/phpMyAdmin, updated by
 * dnf). Fedora's /etc/phpMyAdmin/config.inc.php loads this file when
 * phpMyAdmin runs under LAMPPost. Like XAMPP, it logs in automatically as
 * the database user "root" without a password - only reachable locally.
 */

$cfg['Servers'][1]['verbose'] = 'LAMPPost MariaDB';
$cfg['Servers'][1]['host'] = 'localhost';
$cfg['Servers'][1]['connect_type'] = 'socket';
$cfg['Servers'][1]['socket'] = '@PREFIX@/tmp/mysql.sock';
$cfg['Servers'][1]['auth_type'] = 'config';
$cfg['Servers'][1]['user'] = 'root';
$cfg['Servers'][1]['password'] = '';
$cfg['Servers'][1]['AllowNoPassword'] = true;

$cfg['TempDir'] = '@PREFIX@/tmp/phpmyadmin';
$cfg['UploadDir'] = '';
$cfg['SaveDir'] = '';
$cfg['VersionCheck'] = false;
