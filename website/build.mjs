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
const bytes=await readFile(path.join(source,installer));
const checksum=createHash('sha256').update(bytes).digest('hex');
if(bytes[0]!==0x4d || bytes[1]!==0x5a) throw new Error('Installer is not a Windows executable');
const dist=path.join(root,'dist');await mkdir(dist,{recursive:true});
await cp(path.join(root,'src'),dist,{recursive:true});
await mkdir(path.join(dist,'downloads'),{recursive:true});
const approved=[installer,portable,installer+'.sha256',portable+'.sha256'];
for(const name of approved) await copyFile(path.join(source,name),path.join(dist,'downloads',name));
// A build must never publish unrelated downloads left beside an earlier build.
const names=await readdir(path.join(dist,'downloads'));
if(names.some(name=>!approved.includes(name))) throw new Error('Stale downloads found; build into a fresh website/dist directory');
for(const [from,to] of [['scroll-studio-icon.png','app-icon.png'],['456c-author.png','456c-author.png'],['Karla.ttf','Karla.ttf'],['Karla-OFL.txt','Karla-OFL.txt']]) await copyFile(path.join(workspace,'assets',from),path.join(dist,'assets',to));
const values={VERSION:version,INSTALLER:installer,PORTABLE:portable,CHECKSUM:checksum,SIZE:(bytes.length/1e6).toFixed(1)};
for(const page of ['index.html','release-notes.html']){
  let html=await readFile(path.join(dist,page),'utf8');
  for(const [key,value]of Object.entries(values))html=html.replaceAll(`{{${key}}}`,value);
  if(/\{\{\w+\}\}/.test(html))throw new Error('Unresolved page placeholder');
  await writeFile(path.join(dist,page),html);
}
for(const name of approved){if((await stat(path.join(dist,'downloads',name))).size>25*1024*1024)throw new Error('Move this download to R2 before deploying: '+name);}
const expected=(await readFile(path.join(source,installer+'.sha256'),'utf8')).split(/\s/)[0];
if(expected!==checksum)throw new Error('Installer checksum does not match');
await writeFile(path.join(dist,'release.json'),JSON.stringify({version,installer,portable,sha256:checksum,bytes:bytes.length},null,2)+'\n');
console.log(`Built Scroll Studio ${version}: ${dist}`);
