<?php
declare(strict_types=1);
require __DIR__.'/../src/studio/runtime.php';
$base=dirname(__DIR__,2);$output=$base.'/target/web-tests';
if(!is_dir($output))mkdir($output,0700,true);
putenv('SCROLL_WORKSPACES='.$output.'/workspaces');
$fixtures=array_merge(glob($base.'/examples/*.scro'),glob($base.'/target/world-review/*.scro'));
foreach($fixtures as $source){
    [$id,$folder]=Scroll\create_workspace('test');
    $story=Scroll\import_archive($source,$folder);
    $story['visual_style'].="\nBrowser round-trip verification.";
    Scroll\export_archive($folder,$story,$output.'/'.basename($source));
    $input=new ZipArchive();$input->open($source);
    $export=new ZipArchive();$export->open($output.'/'.basename($source));
    $original=json_decode($input->getFromName('manifest.json'),true);
    if($original['chapters']!==$story['chapters'])throw new Exception('Chapter metadata changed');
    for($i=0;$i<$input->numFiles;$i++){
        $name=$input->getNameIndex($i);
        if(str_starts_with($name,'assets/') && $input->getFromName($name)!==$export->getFromName($name))throw new Exception('Original image changed');
    }
    echo basename($source).": original images and chapter content preserved\n";
}
// Archives must never write paths supplied by an attacker.
foreach(['../outside','assets/../outside','assets/1.php','manifest.json/'] as $entry){
    $path=$output.'/invalid.scro';$z=new ZipArchive();$z->open($path,ZipArchive::CREATE|ZipArchive::OVERWRITE);
    $z->addFromString('manifest.json',file_get_contents(__DIR__.'/../src/studio/template.json'));$z->addFromString($entry,'bad');$z->close();
    [$id,$folder]=Scroll\create_workspace('test');
    try{Scroll\import_archive($path,$folder);throw new Exception('Unsafe entry accepted: '.$entry);}catch(Scroll\Problem $e){}
}
echo "Unsafe archive entries rejected.\n";

$template=Scroll\decode(file_get_contents(__DIR__.'/../src/studio/template.json'));
$v11=$template;
$v11['version']=11;
$v11['timeline']=[['chapter_id'=>1,'when'=>'Year 12','summary'=>'The gate opens.']];
$upgraded=Scroll\normalize($v11);
if($upgraded['version']!==12) throw new Exception('v11 did not upgrade to format 12');
if($upgraded['timeline'][0]['summary']!=='The gate opens.' || $upgraded['timeline'][0]['when']!=='Year 12') throw new Exception('v11 timeline notes changed');
if(($upgraded['timeline'][0]['event'] ?? null)!==null) throw new Exception('v11 gained an event');
if($upgraded['chapters']!==$template['chapters']) throw new Exception('v11 chapters changed');
[$id,$folder]=Scroll\create_workspace('test');
Scroll\validate($upgraded,$folder);
$eventId=$upgraded['next_id']++;
$upgraded['timeline'][]=['chapter_id'=>0,'when'=>'Three years earlier','summary'=>'The citadel burned.','event'=>['id'=>$eventId,'title'=>'The fire','kind'=>'Flashback','links'=>[]]];
Scroll\validate($upgraded,$folder);
$exported=$output.'/events-v12.scro';
Scroll\export_archive($folder,$upgraded,$exported);
[$id2,$folder2]=Scroll\create_workspace('test');
$again=Scroll\import_archive($exported,$folder2);
if($again['version']!==12 || $again['timeline'][1]['event']['title']!=='The fire' || $again['timeline'][1]['event']['kind']!=='Flashback') throw new Exception('v12 event did not round-trip');
if($again['timeline'][0]['summary']!=='The gate opens.') throw new Exception('Chapter chronology changed during event round-trip');
$bad=$again;
$bad['timeline'][1]['event']['kind']='Secret';
try{Scroll\validate($bad,$folder2);throw new Exception('Unknown event kind accepted');}catch(Scroll\Problem $e){}
$bad=$again;
$bad['timeline'][1]['event']['links']=[999];
try{Scroll\validate($bad,$folder2);throw new Exception('Dangling event link accepted');}catch(Scroll\Problem $e){}
try{Scroll\normalize(['version'=>13]+$template);throw new Exception('Future version accepted');}catch(Scroll\Problem $e){}
echo "Format 11 upgrades, format 12 events round-trip, and invalid events are rejected.\n";

