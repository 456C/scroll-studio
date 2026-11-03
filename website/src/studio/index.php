<?php
declare(strict_types=1);
header('X-Content-Type-Options: nosniff');
header('Cache-Control: no-cache');
?><!doctype html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width,initial-scale=1,viewport-fit=cover">
<meta name="theme-color" content="#202124">
<meta name="description" content="Your Scroll Studio world, wherever you are. Edit characters, locations, references and lore, then take your .scro file back to the desktop.">
<title>Assets — Scroll Studio</title>
<link rel="icon" href="/assets/app-icon.png">
<link rel="stylesheet" href="/studio/studio.css">
<script type="module" src="/studio/app.js"></script>
</head>
<body>
<div id="app" class="app"><div class="loading"><img src="/assets/app-icon.png" alt="" width="56" height="56"><span>Opening the studio…</span></div></div>
<div id="overlay"></div><div id="toast" role="status" aria-live="polite"></div>
<input id="open-file" type="file" accept=".scro" hidden>
<input id="image-file" type="file" accept="image/png,image/jpeg,image/webp" multiple hidden>
<iframe name="download-frame" title="File download" hidden></iframe>
<noscript>This editor needs JavaScript for interactive sheets and the lore board. The <a href="/">desktop app</a> works offline.</noscript>
</body></html>

