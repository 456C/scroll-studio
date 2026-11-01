// Render only: the caller must inspect DNS and validate the full server config before enabling.
import {mkdir,writeFile} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const [domain,mode='http']=process.argv.slice(2);
const label='[a-z0-9](?:[a-z0-9-]{0,61}[a-z0-9])?';
if(!domain||domain.length>253||!new RegExp(`^(?:${label}\\.)+[a-z]{2,63}$`).test(domain))throw new Error('Supply a lowercase public hostname, without a scheme or path');
if(!['http','https'].includes(mode))throw new Error('Mode must be http or https');
const security=`add_header X-Content-Type-Options "nosniff" always;
add_header Referrer-Policy "strict-origin-when-cross-origin" always;
add_header Content-Security-Policy "default-src 'self'; img-src 'self' data:; style-src 'self'; script-src 'self'; font-src 'self'; object-src 'none'; base-uri 'self'; form-action 'none'" always;`;
const challenge=`location ^~ /.well-known/acme-challenge/ {
    root /var/www/scroll-studio/acme;
    default_type text/plain;
    try_files $uri =404;
}`;
const content=`server_name ${domain};
root /var/www/scroll-studio/current;
index index.html;
server_tokens off;
access_log off;
error_log /var/log/nginx/scroll-studio-error.log warn;
${security}
add_header Cache-Control "no-cache" always;
${challenge}
location ~ /\\. { return 404; }
location = /SHA256SUMS { return 404; }
location = /_headers { return 404; }
location / { try_files $uri $uri/ =404; }
location ^~ /downloads/ {
    try_files $uri =404;
    ${security.split('\n').join('\n    ')}
    add_header Content-Disposition "attachment";
    add_header Cache-Control "public, max-age=31536000, immutable";
}
error_page 404 /404.html;`;
const config=mode==='http'?`server {
listen 80;
listen [::]:80;
${content}
}
`:`server {
listen 80;
listen [::]:80;
server_name ${domain};
server_tokens off;
access_log off;
${challenge}
location / { return 301 https://${domain}$request_uri; }
}
server {
listen 443 ssl;
listen [::]:443 ssl;
ssl_certificate /etc/letsencrypt/live/${domain}/fullchain.pem;
ssl_certificate_key /etc/letsencrypt/live/${domain}/privkey.pem;
ssl_protocols TLSv1.2 TLSv1.3;
${content}
}
`;
const output=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'../../target/contabo-deploy');
await mkdir(output,{recursive:true});
const destination=path.join(output,`scroll-studio-${mode}.conf`);
await writeFile(destination,config);
console.log(destination);
