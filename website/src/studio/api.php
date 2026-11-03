<?php
declare(strict_types=1);
require __DIR__.'/runtime.php';
use function Scroll\{require_ok,json,decode,root,reserve,create_workspace,workspace,import_archive,validate,save_state,state,image_info,thumbnail,export_archive,normalize};
use Scroll\Problem;
ini_set('display_errors','0');
header('X-Content-Type-Options: nosniff');
header('Cache-Control: no-store');
header("Content-Security-Policy: default-src 'none'; frame-ancestors 'self'");
header('Referrer-Policy: no-referrer');
function respond(array $data,int $status=200): never {
    http_response_code($status);header('Content-Type: application/json; charset=utf-8');echo json($data);exit;
}
function start_session(): void {
    session_name('scroll_world');
    session_start(['cookie_path'=>'/studio/','cookie_httponly'=>true,'cookie_secure'=>isset($_SERVER['HTTPS']) && $_SERVER['HTTPS']!=='off','cookie_samesite'=>'Strict','cookie_lifetime'=>172800,'use_strict_mode'=>true]);
}
function uploaded(string $field,int $limit): string {
    $f=$_FILES[$field] ?? null;
    require_ok(is_array($f) && $f['error']===UPLOAD_ERR_OK && is_uploaded_file($f['tmp_name']), 'Upload failed or exceeds the file limit.');
    require_ok($f['size']>0 && $f['size']<=$limit,'File is too large.');
    return $f['tmp_name'];
}
function remember(string $project): void {
    start_session();$_SESSION['projects']=array_slice(array_values(array_unique(array_merge([$project],$_SESSION['projects'] ?? []))),0,8);session_write_close();
}
$folder=null;$lock=null;
try {
    start_session();
    $_SESSION['owner'] ??= bin2hex(random_bytes(32));
    $_SESSION['csrf'] ??= bin2hex(random_bytes(32));
    $owner=$_SESSION['owner'];$csrf=$_SESSION['csrf'];$recent=$_SESSION['projects'] ?? [];
    session_write_close();
    $action=$_GET['action'] ?? '';
    $method=$_SERVER['REQUEST_METHOD'];
    if ($method==='POST') {
        $origin=$_SERVER['HTTP_ORIGIN'] ?? '';
        $host=$_SERVER['HTTP_HOST'] ?? '';
        if($origin!=='') require_ok(parse_url($origin,PHP_URL_HOST)===explode(':',$host)[0],'Cross-site requests are not allowed.',403);
        require_ok(hash_equals($csrf,(string)($_SERVER['HTTP_X_CSRF_TOKEN'] ?? $_POST['csrf'] ?? '')),'Session changed. Refresh and try again.',403);
    }
    if($action==='session' && $method==='GET'){
        $worlds=[];
        foreach($recent as $id) {
            try {$dir=workspace($id,$owner);$v=state($dir);$worlds[]=['id'=>$id,'title'=>$v['story']['title']];}
            catch(Problem $e){}
        }
        respond(['csrf'=>$csrf,'recent'=>$worlds,'version'=>Scroll\VERSION,'maxFile'=>Scroll\MAX_TOTAL,'expiresIn'=>Scroll\TTL]);
    }
    if(in_array($action,['create','import'],true) && $method==='POST'){
        $source=$action==='import'?uploaded('file',Scroll\MAX_TOTAL):null;
        $capacity=reserve($source?Scroll\MAX_TOTAL:65536);
        [$id,$dir]=create_workspace($owner);
        try {
            $story=$source?import_archive($source,$dir):normalize(decode(file_get_contents(__DIR__.'/template.json')));
            if(!$source)$story['title']='Untitled world';
            validate($story,$dir);save_state($dir,$story,0);
        } catch(\Throwable $e) {Scroll\remove_workspace($dir);throw $e;}
        finally {flock($capacity,LOCK_UN);fclose($capacity);}
        remember($id);respond(['id'=>$id,'story'=>$story,'revision'=>0]);
    }
    $project=(string)($_GET['project'] ?? $_POST['project'] ?? '');
    $folder=workspace($project,$owner);
    if($action==='load' && $method==='GET')respond(['id'=>$project]+state($folder));
    if($action==='image' && $method==='GET'){
        $id=(string)($_GET['id'] ?? '');
        require_ok((bool)preg_match('/^[1-9][0-9]{0,15}$/D',$id),'Image not found.',404);
        $source=$folder.'/assets/'.$id.'.bin';require_ok(is_file($source),'Image not found.',404);
        $size=$_GET['size'] ?? '384';
        if($size!=='original'){
            require_ok(in_array($size,['384','768','1280'],true),'Invalid preview size.');
            $preview=$folder.'/thumbs/'.$id.'-'.$size.'.webp';
            if(!is_file($preview)){
                $previewLock=fopen($folder.'/thumbs/lock-'.$id,'c');
                flock($previewLock,LOCK_EX);
                try {if(!is_file($preview)){$previewCapacity=reserve(6291456);try{thumbnail($source,$preview,(int)$size);}finally{flock($previewCapacity,LOCK_UN);fclose($previewCapacity);}}}
                finally {flock($previewLock,LOCK_UN);fclose($previewLock);}
            }
            $source=$preview;
        }
        $info=image_info($source);header('Content-Type: '.$info['mime']);header('Cache-Control: private, max-age=3600');header('Content-Length: '.filesize($source));
        if(isset($_GET['download']))header('Content-Disposition: attachment; filename="reference-'.$id.'.'.(['image/png'=>'png','image/jpeg'=>'jpg','image/webp'=>'webp'][$info['mime']]).'"');
        readfile($source);exit;
    }
    require_ok($method==='POST','Not found.',404);
    if($action==='upload') {
        $source=uploaded('file',Scroll\MAX_IMAGE);$info=image_info($source);
        $capacity=reserve(filesize($source)+4*1024*1024);
    }
    if($action==='fork')$capacity=reserve(Scroll\MAX_TOTAL);
    $lock=fopen($folder.'/lock','c');require_ok($lock!==false && flock($lock,LOCK_EX),'Workspace busy.',503);
    $current=state($folder);
    if($action==='save' || $action==='fork'){
        $payload=decode(file_get_contents('php://input',false,null,0,Scroll\MAX_MANIFEST+1));
        if($action==='save')require_ok(($payload['revision'] ?? null)===$current['revision'],'Another tab changed this world. Reload before saving.',409);
        $story=$payload['story'] ?? null;require_ok(is_array($story),'Missing world details.');
        // World editing never replaces chapter art, scripts, lettering or unknown story data.
        $next=$current['story'];
        foreach(['title','synopsis','visual_style','assets','locations','groups','timeline','lore','lore_board','card_thumbnails'] as $key) {
            require_ok(array_key_exists($key,$story),'Missing world section.');$next[$key]=$story[$key];
        }
        require_ok(Scroll\identifier($story['next_id'] ?? null),'Invalid world identifier.');
        $next['next_id']=max($current['story']['next_id'],$story['next_id']);
        // Existing image dimensions describe immutable originals, not preview sizes.
        $existing=array_column($current['story']['assets'],null,'id');
        foreach($next['assets'] as $a) if(isset($existing[$a['id']]) && is_file($folder.'/assets/'.$a['id'].'.bin')){
            $old=$existing[$a['id']];
            require_ok($a['width']===$old['width'] && $a['height']===$old['height'],'Original image dimensions cannot be changed.');
        }
        validate($next,$folder);
        if($action==='fork'){
            [$newId,$newDir]=create_workspace($owner);
            try{
                foreach($next['assets'] as $a) if($a['width']>0) require_ok(copy($folder.'/assets/'.$a['id'].'.bin',$newDir.'/assets/'.$a['id'].'.bin'),'Could not preserve an original image.',503);
                $next['title']=mb_safe_cut($next['title'],900).' — copy';
                save_state($newDir,$next,0);
            }catch(\Throwable $e){Scroll\remove_workspace($newDir);throw $e;}
            finally{flock($capacity,LOCK_UN);fclose($capacity);}
            remember($newId);respond(['id'=>$newId,'story'=>$next,'revision'=>0]);
        }
        $revision=$current['revision']+1;save_state($folder,$next,$revision);
        respond(['revision'=>$revision,'next_id'=>$next['next_id']]);
    }
    if($action==='upload'){
        try {
            require_ok((int)($_POST['revision'] ?? -1)===$current['revision'],'Another tab changed this world. Reload before importing.',409);
            $story=$current['story'];$id=$story['next_id']++;
            while(is_file($folder.'/assets/'.$id.'.bin'))$id=$story['next_id']++;
            $kind=(string)($_POST['kind'] ?? 'Image');require_ok(in_array($kind,['Image','Background'],true),'Invalid reference type.');
            $name=substr(pathinfo((string)$_FILES['file']['name'],PATHINFO_FILENAME),0,300);
            $asset=['id'=>$id,'name'=>$name?:'Reference','kind'=>$kind,'notes'=>'','prompt'=>'','width'=>$info[0],'height'=>$info[1],'character'=>Scroll\profile()];
            require_ok(move_uploaded_file($source,$folder.'/assets/'.$id.'.bin'),'Could not store the original image.',503);
            $story['assets'][]=$asset;validate($story,$folder);$revision=$current['revision']+1;save_state($folder,$story,$revision);
            respond(['asset'=>$asset,'revision'=>$revision,'next_id'=>$story['next_id']]);
        } finally {flock($capacity,LOCK_UN);fclose($capacity);}
    }
    if($action==='export'){
        require_ok((int)($_POST['revision'] ?? -1)===$current['revision'],'Your latest edits are not saved yet. Try Export again.',409);
        $path=tempnam(root(),'export-');
        register_shutdown_function(static function()use($path){if(is_file($path))unlink($path);});
        export_archive($folder,$current['story'],$path);
        $filename=preg_replace('/[^\p{L}\p{N} _.-]/u','',$current['story']['title']) ?: 'World';
        $filename=mb_safe_cut($filename,120).'.scro';
        header('Content-Type: application/octet-stream');header('Content-Disposition: attachment; filename="World.scro"; filename*=UTF-8\'\''.rawurlencode($filename));
        header('Content-Length: '.filesize($path));readfile($path);exit;
    }
    throw new Problem('Not found.',404);
} catch(Problem $e){respond(['error'=>$e->getMessage()],$e->status);}
catch(\Throwable $e){error_log('Scroll world: '.get_class($e).': '.$e->getMessage());respond(['error'=>'The editor could not complete that request. Your original file is unchanged.'],500);}
finally {if(is_resource($lock)){flock($lock,LOCK_UN);fclose($lock);}}
function mb_safe_cut(string $s,int $n): string {return preg_replace('/[\x00-\x1F\x7F]/','',implode('',array_slice(preg_split('//u',$s,-1,PREG_SPLIT_NO_EMPTY)?:[],0,$n)));}

