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

