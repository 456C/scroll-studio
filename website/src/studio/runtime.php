<?php
declare(strict_types=1);
// Only index.php and api.php are routed through FastCGI; this library is never public.
namespace Scroll;
use RuntimeException;
use ZipArchive;

const VERSION = 12;
const MAX_TOTAL = 536870912;
const MAX_IMAGE = 67108864;
const MAX_MANIFEST = 16777216;
const TTL = 86400;
const GLOBAL_QUOTA = 4294967296;
const MAX_SAFE_ID = 9007199254740991;
final class Problem extends RuntimeException {
    public function __construct(string $message, public int $status = 422) { parent::__construct($message); }
}
function require_ok(bool $condition, string $message, int $status = 422): void {
    if (!$condition) throw new Problem($message, $status);
}
function root(): string {
    $path = getenv('SCROLL_WORKSPACES') ?: sys_get_temp_dir() . '/scroll-studio-web';
    if (!is_dir($path) && !mkdir($path, 0700, true) && !is_dir($path)) throw new Problem('Workspace unavailable.', 503);
    return $path;
}
function atomic(string $path, string $bytes): void {
    $temp = $path . '.' . bin2hex(random_bytes(6)) . '.tmp';
    try {
        if (file_put_contents($temp, $bytes, LOCK_EX) !== strlen($bytes) || !rename($temp, $path))
            throw new Problem('Could not save. Export your work and try again.', 503);
    } finally { if (is_file($temp)) unlink($temp); }
}
function json(array $data): string {
    return json_encode($data, JSON_THROW_ON_ERROR | JSON_UNESCAPED_UNICODE | JSON_UNESCAPED_SLASHES | JSON_PRESERVE_ZERO_FRACTION);
}
function decode(string $bytes): array {
    require_ok(strlen($bytes) <= MAX_MANIFEST, 'World details exceed 16 MiB.');
    try { $data = json_decode($bytes, true, 256, JSON_THROW_ON_ERROR); }
    catch (\JsonException $e) { throw new Problem('The world manifest is not valid JSON.'); }
    require_ok(is_array($data) && !array_is_list($data), 'Invalid world manifest.');
    return $data;
}
function identifier(mixed $id): bool { return is_int($id) && $id > 0 && $id <= MAX_SAFE_ID; }
function color(mixed $value, int $length = 3): bool {
    return is_array($value) && count($value) === $length
        && count(array_filter($value, fn($v) => is_int($v) && $v >= 0 && $v <= 255)) === $length;
}
function list_of(mixed $value, int $max, string $name): array {
    require_ok(is_array($value) && array_is_list($value) && count($value) <= $max, 'Invalid ' . $name . '.');
    return $value;
}
function text(mixed $value, int $max = 1000000): bool { return is_string($value) && strlen($value) <= $max; }
function profile(): array {
    return array_fill_keys(['tagline','role','age','pronouns','personality','backstory','relationships','face','hair','eyes','skin','build','distinguishing_features','consistency','avoid'], '')
        + ['gender'=>null,'outfits'=>[],'active_outfit'=>null,'palette'=>[],'references'=>[]];
}
function normalize(array $s): array {
    require_ok(is_int($s['version'] ?? null) && $s['version'] >= 1 && $s['version'] <= VERSION,
        'This .scro version is not supported. This editor reads versions 1 through '.VERSION.'.');
    $old = $s['version']; $s['version'] = VERSION;
    foreach (['assets','locations','groups','timeline','lore','bubble_presets','card_thumbnails'] as $key) $s[$key] ??= [];
    $s['visual_style'] ??= ''; $s['synopsis'] ??= '';
    $s['lore_board'] ??= ['cards'=>[], 'connections'=>[]];
    foreach ($s['assets'] as &$a) { $a['character'] = ($a['character'] ?? []) + profile(); }
    unset($a);
    foreach ($s['locations'] as &$l) {
        $l['asset_ids'] ??= [];
        $l['profile'] = ($l['profile'] ?? []) + array_fill_keys(['description','atmosphere','lighting','consistency','notes','prompt','avoid'], '');
    }
    unset($l);
    foreach ($s['groups'] as &$g) {
        $g += ['description'=>'','reference_ids'=>[],'character_ids'=>[],'location_ids'=>[]];
    }
    unset($g);
    foreach ($s['timeline'] as &$entry) {
        if (is_array($entry) && isset($entry['event']) && is_array($entry['event'])) {
            $entry['event']['links'] ??= [];
        }
    }
    unset($entry);
    require_ok(identifier($s['next_id'] ?? null), 'Invalid or too-large world identifier.');
    if ($old < 11 && !$s['lore_board']['cards']) {
        $targets = [];
        foreach ($s['lore'] as $n) {
            $targets[$n['id']] = true;
            foreach ($n['links'] as $id) $targets[$id] = true;
        }
        $map = []; $i = 0;
        foreach (array_keys($targets) as $id) {
            $cardId = $s['next_id']++;
            $map[$id] = $cardId;
            $s['lore_board']['cards'][] = ['id'=>$cardId,'target'=>$id,'position'=>[($i%4)*264, intdiv($i,4)*300],'note'=>'','color'=>[222,181,105]];
            $i++;
        }
        $pairs = [];
        foreach ($s['lore'] as $n) foreach ($n['links'] as $id) {
            require_ok(isset($map[$id]), 'A lore reference is missing.');
            $from=$map[$n['id']];$to=$map[$id];$pair=min($from,$to).':'.max($from,$to);
            if ($from === $to || isset($pairs[$pair])) continue;
            $pairs[$pair]=true;
            $s['lore_board']['connections'][]=['id'=>$s['next_id']++,'from'=>$from,'to'=>$to,'label'=>'','color'=>[190,103,106]];
        }
    }
    return $s;
}
function validate(array $s, string $folder): void {
    require_ok(text($s['title'] ?? null,4096) && text($s['synopsis'] ?? null) && text($s['visual_style'] ?? null), 'Invalid story details.');
    require_ok(($s['version'] ?? null) === VERSION && identifier($s['next_id'] ?? null), 'Invalid world version or identifier.');
    $ids=[];$maximum=0;
    $claim=function($id) use (&$ids,&$maximum): void {
        require_ok(identifier($id) && !isset($ids[$id]), 'Duplicate, invalid, or too-large identifier.');
        $ids[$id]=true;$maximum=max($maximum,$id);
    };
    $chapters=list_of($s['chapters'] ?? null,1000,'chapters');
    require_ok(count($chapters)>0, 'A desktop-compatible world needs a chapter.');
    $chapterIds=[];$protectedAssets=[];
    foreach ($chapters as $c) {
        $claim($c['id'] ?? null);$chapterIds[$c['id']]=true;
        foreach ($c['panels'] ?? [] as $p) $claim($p['id'] ?? null);
        foreach ($c['layers'] ?? [] as $l) {
            $claim($l['id'] ?? null);
            if (isset($l['kind']['Image']['asset_id'])) $protectedAssets[$l['kind']['Image']['asset_id']]=true;
        }
    }
    foreach (list_of($s['bubble_presets'],4096,'bubble presets') as $p) $claim($p['id'] ?? null);
    $assets=[];$images=[];$allImages=[];$characters=[];$backgrounds=[];$totalBytes=0;
    foreach (list_of($s['assets'],4096,'assets') as $a) {
        $claim($a['id'] ?? null);
        require_ok(text($a['name'] ?? null,4096) && text($a['notes'] ?? null) && text($a['prompt'] ?? null), 'Invalid asset details.');
        require_ok(in_array($a['kind'] ?? null,['Character','Background','Image'],true), 'Unknown asset type.');
        require_ok(is_int($a['width'] ?? null) && is_int($a['height'] ?? null), 'Invalid image size.');
        $w=$a['width'];$h=$a['height'];
        require_ok(($w===0 && $h===0) || ($w>0 && $h>0 && $w<=16384 && $h<=16384 && $w*$h<=32000000 && is_file($folder.'/assets/'.$a['id'].'.bin')), 'An original image is missing or too large.');
        $assets[$a['id']]=$a;
        if($w>0){$allImages[$a['id']]=true;$totalBytes+=filesize($folder.'/assets/'.$a['id'].'.bin');}
        require_ok($totalBytes<=MAX_TOTAL-MAX_MANIFEST,'Original images exceed the .scro size limit.');
        if ($a['kind']==='Character') $characters[$a['id']]=true;
        if ($a['kind']==='Image' && $w>0) $images[$a['id']]=true;
        if ($a['kind']==='Background') $backgrounds[$a['id']]=true;
        $p=($a['character'] ?? []) + profile();
        foreach (profile() as $key=>$value) if (is_string($value)) require_ok(text($p[$key]),'Invalid character detail.');
        require_ok(in_array($p['gender'],[null,'Female','Male'],true),'Invalid character gender.');
        $outfits=list_of($p['outfits'],32,'outfits');
        foreach ($outfits as $o) require_ok(text($o['name'] ?? null,4096) && text($o['description'] ?? null),'Invalid outfit.');
        require_ok($p['active_outfit']===null || (is_int($p['active_outfit']) && isset($outfits[$p['active_outfit']])),'Selected outfit is missing.');
        foreach (list_of($p['palette'],16,'palette') as $sw) require_ok(text($sw['name'] ?? null,4096) && color($sw['color'] ?? null,4),'Invalid palette color.');
    }
    foreach ($protectedAssets as $id=>$_) require_ok(isset($assets[$id]),'An image used by a chapter cannot be removed.');
    $refs=function($values,$allowed,$label,$limit=4096): void {
        $seen=[];
        foreach (list_of($values,$limit,$label) as $id) {
            require_ok(identifier($id) && isset($allowed[$id]) && !isset($seen[$id]),'A '.$label.' reference is missing or duplicated.');
            $seen[$id]=true;
        }
    };
    foreach ($assets as $a) {
        $references=list_of($a['character']['references'] ?? [],24,'character references');
        require_ok(!$references || $a['kind']==='Character','Only characters can own character references.');
        $rids=[];
        foreach ($references as $r) {
            require_ok(text($r['label'] ?? null,4096) && text($r['notes'] ?? null),'Invalid reference details.');
            $rids[]=$r['asset_id'] ?? null;
        }
        $refs($rids,$images,'character',24);
    }
    $locations=[];$groupIds=[];$noteIds=[];
    foreach (list_of($s['locations'],4096,'locations') as $l) {
        $claim($l['id'] ?? null);$locations[$l['id']]=true;
        require_ok(text($l['name'] ?? null,4096) && trim($l['name'])!=='','Invalid location name.');
        $refs($l['asset_ids'] ?? [],$backgrounds,'background');
        foreach (($l['profile'] ?? []) as $key=>$value) require_ok(text($value),'Invalid location details.');
    }
    $owned=[];
    foreach ($s['locations'] as $l) foreach ($l['asset_ids'] as $id) {
        require_ok(!isset($owned[$id]),'A background can belong to only one location.');$owned[$id]=true;
    }
    foreach (list_of($s['groups'],4096,'groups') as $g) {
        $claim($g['id'] ?? null);$groupIds[$g['id']]=true;
        require_ok(text($g['name'] ?? null,4096) && trim($g['name'])!=='' && text($g['description'] ?? null,20000) && color($g['color'] ?? null),'Invalid group.');
        $refs($g['reference_ids'] ?? [],$allImages,'group image',64);
        $refs($g['character_ids'] ?? [],$characters,'group character');
        $refs($g['location_ids'] ?? [],$locations,'group location');
    }
    foreach (list_of($s['lore'],4096,'lore notes') as $n) {
        $claim($n['id'] ?? null);$noteIds[$n['id']]=true;
        require_ok(text($n['title'] ?? null,4096) && text($n['text'] ?? null,100000),'Invalid lore note.');
    }
    $targets=$characters+$locations+$groupIds+$noteIds;
    foreach ($s['lore'] as $n) { $allowed=$targets;unset($allowed[$n['id']]);$refs($n['links'] ?? [],$allowed,'lore'); }
    $covers=[];
    foreach(list_of($s['card_thumbnails'] ?? [],12288,'card thumbnails') as $cover){
        $target=$cover['target'] ?? 0;$image=$cover['asset_id'] ?? 0;
        require_ok(identifier($target)&&identifier($image)&&!isset($covers[$target])&&isset($allImages[$image]),'Invalid card thumbnail.');
        $allowed=[];
        if(isset($characters[$target])){$a=$assets[$target];$allowed=array_column($a['character']['references'],'asset_id');if($a['width']>0)$allowed[]=$target;}
        foreach($s['locations'] as $l)if($l['id']===$target)$allowed=$l['asset_ids'];
        foreach($s['groups'] as $g)if($g['id']===$target)$allowed=$g['reference_ids'];
        require_ok(in_array($image,$allowed,true),'Choose a thumbnail from this card’s references.');$covers[$target]=true;
    }
    $cards=[];$pinned=[];
    foreach (list_of($s['lore_board']['cards'] ?? null,8192,'lore cards') as $c) {
        $claim($c['id'] ?? null);$cards[$c['id']]=true;
        require_ok(isset($targets[$c['target'] ?? 0]) && !isset($pinned[$c['target']]),'Invalid lore card target.');$pinned[$c['target']]=true;
        require_ok(text($c['note'] ?? null,100000) && color($c['color'] ?? null),'Invalid board note.');
        $position=$c['position'] ?? null;
        require_ok(is_array($position) && count($position)===2 && (is_int($position[0]) || is_float($position[0])) && (is_int($position[1]) || is_float($position[1])) && is_finite((float)$position[0]) && is_finite((float)$position[1]) && abs($position[0])<=1000000 && abs($position[1])<=1000000,'Invalid card position.');
    }
    $pairs=[];
    foreach (list_of($s['lore_board']['connections'] ?? null,16384,'connections') as $e) {
        $claim($e['id'] ?? null);$a=$e['from'] ?? 0;$b=$e['to'] ?? 0;$pair=min($a,$b).':'.max($a,$b);
        require_ok(isset($cards[$a],$cards[$b]) && $a!==$b && !isset($pairs[$pair]),'Invalid board connection.');$pairs[$pair]=true;
        require_ok(text($e['label'] ?? null,4096) && color($e['color'] ?? null),'Invalid relationship label.');
    }
    $seen=[];$seenEvents=[];$events=0;
    $linkTargets=$characters+$locations+$groupIds;
    foreach (list_of($s['timeline'],count($chapters)+4096,'timeline') as $e) {
        require_ok(text($e['when'] ?? null,1024) && text($e['summary'] ?? null,20000),'Invalid timeline entry.');
        if (isset($e['event']) && $e['event'] !== null) {
            $events++;
            require_ok($events<=4096,'Too many story events.');
            require_ok(($e['chapter_id'] ?? 0)===0,'A story event cannot replace a chapter.');
            $ev=$e['event'];
            require_ok(is_array($ev) && !array_is_list($ev),'Invalid story event.');
            $claim($ev['id'] ?? null);
            require_ok(!isset($seenEvents[$ev['id']]) && !isset($chapterIds[$ev['id']]),'Timeline contains a missing or duplicate event.');
            $seenEvents[$ev['id']]=true;
            require_ok(text($ev['title'] ?? null,4096) && in_array($ev['kind'] ?? null,['Flashback','Lore','Other'],true),'Invalid story event.');
            $refs($ev['links'] ?? [],$linkTargets,'event link',64);
        } else {
            $id=$e['chapter_id'] ?? 0;
            require_ok(isset($chapterIds[$id]) && !isset($seen[$id]),'Invalid timeline entry.');
            $seen[$id]=true;
        }
    }
    require_ok($s['next_id']>$maximum,'World identifier counter is out of date.');
    require_ok(strlen(json($s))<=MAX_MANIFEST,'World details exceed 16 MiB.');
}
function remove_workspace(string $directory): void {
    if (!preg_match('/^[a-f0-9]{32}$/D',basename($directory)) || dirname($directory)!==root()) return;
    foreach (new \RecursiveIteratorIterator(new \RecursiveDirectoryIterator($directory,\FilesystemIterator::SKIP_DOTS),\RecursiveIteratorIterator::CHILD_FIRST) as $file) {
        if ($file->isLink() || $file->isFile()) unlink($file->getPathname()); else rmdir($file->getPathname());
    }
    rmdir($directory);
}
function cleanup(): void {
    foreach (glob(root().'/*/owner.json') ?: [] as $owner) {
        if (filemtime($owner)<time()-TTL) {
            $dir=dirname($owner);$lock=fopen($dir.'/lock','c');
            if ($lock && flock($lock,LOCK_EX|LOCK_NB)) { remove_workspace($dir); flock($lock,LOCK_UN); }
            if ($lock) fclose($lock);
        }
    }
}
function storage_bytes(): int {
    $total=0;
    foreach (new \RecursiveIteratorIterator(new \RecursiveDirectoryIterator(root(),\FilesystemIterator::SKIP_DOTS)) as $f) if ($f->isFile()) $total += $f->getSize();
    return $total;
}
function reserve(int $bytes): mixed {
    $lock=fopen(root().'/capacity.lock','c');require_ok($lock!==false && flock($lock,LOCK_EX),'Workspace busy.',503);
    cleanup();
    if (storage_bytes()+$bytes>GLOBAL_QUOTA || disk_free_space(root()) < $bytes+536870912) {
        fclose($lock);throw new Problem('The editor is busy. Please try importing again later.',503);
    }
    return $lock;
}
function create_workspace(string $owner): array {
    $id=bin2hex(random_bytes(16));$dir=root().'/'.$id;
    mkdir($dir,0700);mkdir($dir.'/assets',0700);mkdir($dir.'/thumbs',0700);
    atomic($dir.'/owner.json',json(['owner'=>$owner,'created'=>time()]));
    return [$id,$dir];
}
function workspace(string $id,string $owner): string {
    require_ok((bool)preg_match('/^[a-f0-9]{32}$/D',$id),'Workspace not found.',404);
    $dir=root().'/'.$id;
    require_ok(is_file($dir.'/owner.json') && filemtime($dir.'/owner.json')>=time()-TTL,'Workspace expired. Reopen your .scro file.',410);
    $info=decode(file_get_contents($dir.'/owner.json'));
    require_ok(hash_equals($info['owner'],$owner),'Workspace not found.',404);
    touch($dir.'/owner.json');
    return $dir;
}
function image_info(string $path): array {
    $info=@getimagesize($path);
    require_ok($info!==false && in_array($info['mime'],['image/png','image/jpeg','image/webp'],true),'Choose a PNG, JPEG, or WebP image.');
    require_ok($info[0]>0 && $info[1]>0 && $info[0]<=16384 && $info[1]<=16384 && $info[0]*$info[1]<=32000000,'Image exceeds 32 megapixels or 16,384 pixels on one side.');
    return $info;
}
function import_archive(string $source,string $folder): array {
    require_ok(filesize($source)>0 && filesize($source)<=MAX_TOTAL,'The .scro file exceeds 512 MiB.');
    $zip=new ZipArchive();
    require_ok($zip->open($source,ZipArchive::RDONLY|ZipArchive::CHECKCONS)===true,'This is not a valid .scro archive.');
    try {
        require_ok($zip->numFiles>=1 && $zip->numFiles<=4097,'Too many archive entries.');
        $names=[];$total=0;
        for($i=0;$i<$zip->numFiles;$i++){
            $e=$zip->statIndex($i);$name=$e['name'];$manifest=$name==='manifest.json';
            require_ok(($manifest || preg_match('/^assets\/[1-9][0-9]{0,15}\.bin$/D',$name)) && !isset($names[$name]),'Unexpected or duplicate archive entry.');
            require_ok($e['size']<=($manifest?MAX_MANIFEST:MAX_IMAGE) && ($e['encryption_method'] ?? 0)===0,'An archive entry is too large or encrypted.');
            $total+=$e['size'];require_ok($total<=MAX_TOTAL,'Archive expands beyond 512 MiB.');$names[$name]=$e;
        }
        require_ok(isset($names['manifest.json']),'The archive has no manifest.');
        $raw=$zip->getFromName('manifest.json');require_ok(is_string($raw),'Cannot read the manifest.');
        $s=normalize(decode($raw));$declared=[];
        foreach ($s['assets'] as $a) {require_ok(identifier($a['id'] ?? null),'Invalid asset identifier.');$declared['assets/'.$a['id'].'.bin']=$a;}
        foreach ($names as $name=>$e) if ($name!=='manifest.json'){
            require_ok(isset($declared[$name]),'Archive image is absent from its manifest.');
            $in=$zip->getStream($name);require_ok(is_resource($in),'Cannot read an original image.');
            $out=fopen($folder.'/'.$name,'wb');$crc=hash_init('crc32b');$copied=0;
            try {
                while(!feof($in)){
                    $block=fread($in,65536);require_ok($block!==false,'Cannot read an original image.');
                    $copied+=strlen($block);require_ok($copied<=$e['size'],'Invalid archive size.');
                    hash_update($crc,$block);require_ok(fwrite($out,$block)===strlen($block),'Could not store the original image.',503);
                }
            } finally {fclose($in);fclose($out);}
            require_ok($copied===$e['size'] && hash_final($crc)===sprintf('%08x',$e['crc']),'An original image is damaged.');
            $info=image_info($folder.'/'.$name);$a=$declared[$name];
            require_ok($info[0]===$a['width'] && $info[1]===$a['height'],'Image dimensions do not match the manifest.');
        }
        validate($s,$folder);return $s;
    } finally {$zip->close();}
}
function save_state(string $folder,array $story,int $revision): void {
    atomic($folder.'/state.json',json(['story'=>$story,'revision'=>$revision]));
}
function state(string $folder): array { return decode(file_get_contents($folder.'/state.json')); }
function export_archive(string $folder,array $story,string $path): void {
    validate($story,$folder);
    $zip=new ZipArchive();require_ok($zip->open($path,ZipArchive::CREATE|ZipArchive::OVERWRITE)===true,'Could not create the download.',503);
    try {
        $zip->addFromString('manifest.json',json($story));
        foreach($story['assets'] as $a) if(is_file($folder.'/assets/'.$a['id'].'.bin')){
            $name='assets/'.$a['id'].'.bin';$zip->addFile($folder.'/'.$name,$name);$zip->setCompressionName($name,ZipArchive::CM_STORE);
        }
    } finally {require_ok($zip->close(),'Could not finish the download.',503);}
}
function thumbnail(string $source,string $destination,int $size): void {
    $info=image_info($source);
    $image=match($info['mime']){'image/png'=>@imagecreatefrompng($source),'image/jpeg'=>@imagecreatefromjpeg($source),'image/webp'=>@imagecreatefromwebp($source)};
    require_ok($image!==false,'Image preview could not be decoded.');
    try {
        $scale=min(1,$size/max($info[0],$info[1]));$w=max(1,(int)round($info[0]*$scale));$h=max(1,(int)round($info[1]*$scale));
        $small=imagecreatetruecolor($w,$h);imagealphablending($small,false);imagesavealpha($small,true);
        imagecopyresampled($small,$image,0,0,0,0,$w,$h,$info[0],$info[1]);
        $temp=$destination.'.'.bin2hex(random_bytes(4)).'.tmp';
        try { require_ok(imagewebp($small,$temp,85),'Preview unavailable.',503);rename($temp,$destination); }
        finally {imagedestroy($small);if(is_file($temp))unlink($temp);}
    } finally {imagedestroy($image);}
}

