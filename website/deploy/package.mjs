// Produce an allowlisted static release; never upload the workspace or its Git metadata.
import '../check.mjs';
import {readFile,writeFile,mkdir,copyFile} from 'node:fs/promises';
import {createHash} from 'node:crypto';
import {spawnSync} from 'node:child_process';
import {fileURLToPath} from 'node:url';
import path from 'node:path';
const website=path.resolve(path.dirname(fileURLToPath(import.meta.url)),'..');
const dist=path.join(website,'dist');
const release=JSON.parse(await readFile(path.join(dist,'release.json'),'utf8'));
if(!/^\d+\.\d+\.\d+$/.test(release.version))throw new Error('Invalid release version');
const installer=`Scroll-Studio-${release.version}-Setup-x64.exe`;
const portable=`Scroll-Studio-${release.version}-windows-x64.zip`;
const files=['index.html','404.html','privacy.html','release-notes.html','release.json','style.css','app.js',
  ...['index.php','api.php','runtime.php','template.json','studio.css','app.js','model.js','icons.js','board.js','camera.js','timeline.js','editing.js'].map(name=>'studio/'+name),
  ...['index.php','_lib/core.php','_lib/media.php','_lib/views.php','_lib/reader.php','_lib/account.php','_lib/creator.php','assets/read.css','assets/read.js'].map(name=>'read/'+name),
  'assets/app-icon.png','assets/456c-author.png','assets/citadel.svg','assets/Karla.ttf','assets/Karla-OFL.txt',
  ...[installer,portable,installer+'.sha256',portable+'.sha256'].map(name=>'downloads/'+name)].sort();
const contents=await Promise.all(files.map(async name=>({name,bytes:await readFile(path.join(dist,name))})));
const manifest=contents.map(({name,bytes})=>`${createHash('sha256').update(bytes).digest('hex')}  ${name}`).join('\n')+'\n';
const contentHash=createHash('sha256').update(manifest).digest('hex');
const releaseId=`v${release.version}-${contentHash.slice(0,12)}`;
const output=path.resolve(website,'../target/contabo-deploy');
const staging=path.join(output,releaseId);
await mkdir(staging,{recursive:true});
for(const {name} of contents){
  const destination=path.join(staging,name);
  await mkdir(path.dirname(destination),{recursive:true});
  await copyFile(path.join(dist,name),destination);
}
await writeFile(path.join(staging,'SHA256SUMS'),manifest);
const archive=path.join(output,`scroll-studio-web-${releaseId}.tar.gz`);
const result=spawnSync('tar',['-czf',archive,'-C',staging,'--',...files,'SHA256SUMS'],{encoding:'utf8',windowsHide:true});
if(result.status!==0)throw new Error(result.stderr||'Archive creation failed');
const archiveBytes=await readFile(archive);
const info={releaseId,archive,sha256:createHash('sha256').update(archiveBytes).digest('hex'),bytes:archiveBytes.length,files:files.length+1};
await writeFile(path.join(output,'package.json'),JSON.stringify(info,null,2)+'\n');
console.log(JSON.stringify(info,null,2));
