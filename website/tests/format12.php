<?php
declare(strict_types=1);
// Format 12: extensions, provenance and unknown layer kinds must pass through import and export unchanged.
require __DIR__.'/../src/studio/runtime.php';
$base=dirname(__DIR__,2);$output=$base.'/target/web-tests';
if(!is_dir($output))mkdir($output,0700,true);
putenv('SCROLL_WORKSPACES='.$output.'/workspaces');
$template=file_get_contents(__DIR__.'/../src/studio/template.json');
function fail(string $message): never { throw new Exception($message); }
function archive(string $path,string $manifest,array $files=[]): string {
    $z=new ZipArchive();$z->open($path,ZipArchive::CREATE|ZipArchive::OVERWRITE);
    $z->addFromString('manifest.json',$manifest);
    foreach($files as $name=>$bytes)$z->addFromString($name,$bytes);
    $z->close();return $path;
}
function manifest_of(string $path): string { $z=new ZipArchive();$z->open($path);return $z->getFromName('manifest.json'); }
function refused(string $path): bool {
    [$id,$folder]=Scroll\create_workspace('test');
    try{Scroll\import_archive($path,$folder);return false;}catch(Scroll\Problem $e){return true;}
}
$png=base64_decode('iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mP8z8BQDwAEhQGAhKmMIQAAAABJRU5ErkJggg==');
$ext=fn(string $name)=>'{"studio.example.'.$name.'":{"flags":{},"list":[1,2.5,{"deep":null}],"text":"ünï","0":"zero","empty":[]},"other":{}}';
$manifest='{"version":12,"title":"F","synopsis":"","visual_style":"","next_id":20,"extensions":'.$ext('story').','
    .'"chapters":[{"id":1,"name":"C","width":100,"height":100,"extensions":'.$ext('chapter').',"panels":[],"layers":['
    .'{"id":2,"extensions":'.$ext('layer').',"kind":{"Image":{"asset_id":5}}},'
    .'{"id":3,"kind":{"Mannequin":{"pose":{"arms":[0.5,1.0],"locked":{}},"rig":"v2"}}}]}],'
    .'"assets":[{"id":5,"name":"A","kind":"Image","notes":"","prompt":"","width":1,"height":1,"extensions":'.$ext('asset').','
    .'"provenance":{"addon":"gen","model":"m1","prompt":"p","seed":42,"sources":[1,2],"future_key":{"x":{}}}}],'
    .'"locations":[{"id":6,"name":"L","extensions":'.$ext('location').'}]}';
$source=archive($output.'/format12-in.scro',$manifest,['assets/5.bin'=>$png]);
[$id,$folder]=Scroll\create_workspace('test');
$story=Scroll\import_archive($source,$folder);
if($story['version']!==12)fail('Format 12 did not import as 12');
$story['title'].=' edited';$story['locations'][0]['name'].=' edited';
// The editor keeps state through JSON between requests; do the same before exporting.
$story=Scroll\decode(Scroll\json($story));
Scroll\export_archive($folder,$story,$output.'/format12-out.scro');
$in=json_decode($manifest);$back=json_decode(manifest_of($output.'/format12-out.scro'));
if($back->version!==12)fail('Export is not format 12');
$pairs=[['story',$in,$back],['chapter',$in->chapters[0],$back->chapters[0]],['layer',$in->chapters[0]->layers[0],$back->chapters[0]->layers[0]],['asset',$in->assets[0],$back->assets[0]],['location',$in->locations[0],$back->locations[0]]];
foreach($pairs as [$label,$a,$b])if(json_encode($a->extensions)!==json_encode($b->extensions))fail('Extensions changed on '.$label);
if(json_encode($in->assets[0]->provenance)!==json_encode($back->assets[0]->provenance))fail('Provenance changed');
if(json_encode($in->chapters[0]->layers[1])!==json_encode($back->chapters[0]->layers[1]))fail('Unknown layer kind changed');
if($back->title!=='F edited' || $back->locations[0]->name!=='L edited')fail('Edit lost');
echo "Format 12 fixture: extensions (5 levels), provenance and unknown layer kind preserved.\n";

// A version 11 file imports and exports as 12 and gains no empty extensions.
$m=json_decode($template,true);$m['version']=11;
$old=archive($output.'/format11-in.scro',json_encode($m));
[$id,$folder]=Scroll\create_workspace('test');
$story=Scroll\import_archive($old,$folder);
if($story['version']!==12)fail('Version 11 did not import as 12');
Scroll\export_archive($folder,$story,$output.'/format11-out.scro');
$text=manifest_of($output.'/format11-out.scro');
if(json_decode($text)->version!==12||str_contains($text,'"extensions"')||str_contains($text,'"provenance"'))fail('Version 11 export is wrong');
echo "Version 11 imports and exports as 12 without empty extensions.\n";
if(json_decode($template)->version!==12)fail('Template is not format 12');

foreach([13,0] as $v){
    $m=json_decode($template,true);$m['version']=$v;
    if(!refused(archive($output.'/bad-version.scro',json_encode($m))))fail('Version '.$v.' accepted');
}
foreach(['[]','"x"','null','{"a":1}'] as $i=>$value){
    // Malformed extensions or provenance at each level are refused; a non-empty object is accepted.
    $m=json_decode($template,true);$raw=substr(json_encode($m),0,-1).',"extensions":'.$value.'}';
    $accepted=!refused(archive($output.'/bad-ext.scro',$raw));
    if($accepted!==($value==='{"a":1}'))fail('Story extensions '.$value.' handled wrongly');
}
foreach(['"extensions":[]','"extensions":"x"'] as $field){
    $bad=str_replace('"extensions":'.$ext('asset'),$field,$manifest);
    if(!refused(archive($output.'/bad-ext.scro',$bad,['assets/5.bin'=>$png])))fail('Accepted asset '.$field);
}
$bad=str_replace('"provenance":{"addon"','"provenance":["addon"',$manifest);
$bad=preg_replace('/"provenance":\["addon".*?"future_key":\{"x":\{\}\}\}/','"provenance":["x"]',$bad);
if(!refused(archive($output.'/bad-prov.scro',$bad,['assets/5.bin'=>$png])))fail('Accepted list provenance');
echo "Version 13 and malformed extensions or provenance refused.\n";
