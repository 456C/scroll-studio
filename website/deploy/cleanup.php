<?php
declare(strict_types=1);
if(PHP_SAPI!=='cli')exit(1);
require '/var/www/scroll-studio/current/studio/runtime.php';
putenv('SCROLL_WORKSPACES=/var/lib/scroll-studio-web/workspaces');
$lock=fopen(Scroll\root().'/capacity.lock','c');
if(!flock($lock,LOCK_EX|LOCK_NB))exit(0);
Scroll\cleanup();
foreach(glob('/var/lib/scroll-studio-web/sessions/sess_*')?:[] as $path)if(is_file($path)&&filemtime($path)<time()-86400)unlink($path);
foreach(glob('/var/lib/scroll-studio-web/workspaces/export-*')?:[] as $path)if(is_file($path)&&filemtime($path)<time()-3600)unlink($path);

