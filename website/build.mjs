import {readFile, writeFile, mkdir, cp, copyFile, stat, readdir} from 'node:fs/promises';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
import {createHash} from 'node:crypto';
const root=path.dirname(fileURLToPath(import.meta.url));
const workspace=path.dirname(root);
const version=(await readFile(path.join(workspace,'Cargo.toml'),'utf8')).match(/^version = "([\d.]+)"/m)[1];
const installer=`Scroll-Studio-${version}-Setup-x64.exe`;
const portable=`Scroll-Studio-${version}-windows-x64.zip`;
const source=path.join(workspace,'dist');
// The AI Lite addon is packaged beside the app (see addons/ai-lite).
const aiLiteManifest=JSON.parse(await readFile(path.join(workspace,'addons','ai-lite','addon.json'),'utf8'));
const aiLiteVersion=(await readFile(path.join(workspace,'addons','ai-lite','Cargo.toml'),'utf8')).match(/^version = "([\d.]+)"/m)[1];
if(aiLiteManifest.version!==aiLiteVersion) throw new Error('addons/ai-lite: addon.json and Cargo.toml versions differ');
const aiLite=`ai-lite-${aiLiteVersion}.zip`;
const aiLiteBytes=await readFile(path.join(source,aiLite));
const aiLiteChecksum=createHash('sha256').update(aiLiteBytes).digest('hex');
if(aiLiteBytes[0]!==0x50 || aiLiteBytes[1]!==0x4b) throw new Error('AI Lite package is not a ZIP');
// Offer the addon only beside an app that can install it.
const semver=text=>text.split('.').map(Number);
const atLeast=(have,need)=>{const [a,b]=[semver(have),semver(need)];for(let i=0;i<3;i++){if(a[i]!==b[i])return a[i]>b[i];}return true;};
const aiLiteOffered=atLeast(version,aiLiteManifest.min_app);
if(!aiLiteOffered) console.warn(`AI Lite needs Scroll Studio ${aiLiteManifest.min_app}; not offered beside ${version}`);
const bytes=await readFile(path.join(source,installer));
const checksum=createHash('sha256').update(bytes).digest('hex');
if(bytes[0]!==0x4d || bytes[1]!==0x5a) throw new Error('Installer is not a Windows executable');
const dist=path.join(root,'dist');await mkdir(dist,{recursive:true});
await cp(path.join(root,'src'),dist,{recursive:true});
await mkdir(path.join(dist,'downloads'),{recursive:true});
const approved=[installer,portable,installer+'.sha256',portable+'.sha256',...(aiLiteOffered?[aiLite,aiLite+'.sha256']:[])];
for(const name of approved) await copyFile(path.join(source,name),path.join(dist,'downloads',name));
// A build must never publish unrelated downloads left beside an earlier build.
const names=await readdir(path.join(dist,'downloads'));
if(names.some(name=>!approved.includes(name))) throw new Error('Stale downloads found; build into a fresh website/dist directory');
for(const [from,to] of [['scroll-studio-icon.png','app-icon.png'],['456c-author.png','456c-author.png'],['Karla.ttf','Karla.ttf'],['Karla-OFL.txt','Karla-OFL.txt']]) await copyFile(path.join(workspace,'assets',from),path.join(dist,'assets',to));
const downloadNote = await readFile(path.join(root, 'download-note.html'), 'utf8');
const escape=text=>String(text).replace(/[&<>"]/g,c=>({'&':'&amp;','<':'&lt;','>':'&gt;','"':'&quot;'}[c]));
const values={VERSION:version,INSTALLER:installer,PORTABLE:portable,CHECKSUM:checksum,SIZE:(bytes.length/1e6).toFixed(1),DOWNLOAD_NOTE:downloadNote,
  AI_LITE_VERSION:aiLiteVersion,AI_LITE_ZIP:aiLite,AI_LITE_CHECKSUM:aiLiteChecksum,AI_LITE_SIZE:(aiLiteBytes.length/1e6).toFixed(1),
  AI_LITE_MIN_APP:escape(aiLiteManifest.min_app),AI_LITE_HARDWARE:escape(aiLiteManifest.hardware),
  AI_LITE_DOWNLOAD:aiLiteOffered
    ?`<a class="button primary" href="downloads/${aiLite}" download>Download AI Lite <span aria-hidden="true">↓</span></a><details><summary>Verify your download</summary><p>SHA-256</p><code>${aiLiteChecksum}</code><a href="downloads/${aiLite}.sha256" download>Download checksum</a></details>`
    :`<p><strong>Arrives with Scroll Studio ${escape(aiLiteManifest.min_app)}.</strong></p>`};
for(const page of ['index.html','release-notes.html','addons.html']){
  let html=await readFile(path.join(dist,page),'utf8');
  for(const [key,value]of Object.entries(values))html=html.replaceAll(`{{${key}}}`,value);
  if(/\{\{\w+\}\}/.test(html))throw new Error('Unresolved page placeholder');
  await writeFile(path.join(dist,page),html);
}
for(const name of approved){if((await stat(path.join(dist,'downloads',name))).size>25*1024*1024)throw new Error('Move this download to R2 before deploying: '+name);}
const expected=(await readFile(path.join(source,installer+'.sha256'),'utf8')).split(/\s/)[0];
if(expected!==checksum)throw new Error('Installer checksum does not match');
if((await readFile(path.join(source,aiLite+'.sha256'),'utf8')).split(/\s/)[0]!==aiLiteChecksum)throw new Error('AI Lite checksum does not match');
await writeFile(path.join(dist,'release.json'),JSON.stringify({version,installer,portable,sha256:checksum,bytes:bytes.length,addons:aiLiteOffered?[{id:aiLiteManifest.id,version:aiLiteVersion,min_app:aiLiteManifest.min_app,file:aiLite,sha256:aiLiteChecksum,bytes:aiLiteBytes.length}]:[]},null,2)+'\n');
console.log(`Built Scroll Studio ${version}: ${dist}`);
