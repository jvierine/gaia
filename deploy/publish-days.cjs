// Run under the publication lock. Image/data transfer only; code delivery is Git.
const fs=require('fs'),path=require('path'),cp=require('child_process');
const root='/mnt/data/juha/gaia',bin='/mnt/data/juha/gaia-build/release';
function run(cmd,args,opts={}){return cp.execFileSync(cmd,args,{stdio:opts.capture?['ignore','pipe','inherit']:'inherit',...opts,encoding:'utf8'})}
const records=run('sqlite3',['-json',root+'/gaia.sqlite3',"SELECT substr(observation_utc,1,10) AS date,count(*) AS images FROM images GROUP BY 1 ORDER BY 1 DESC"],{capture:true});
const days=JSON.parse(records);const ssh=['-o','BatchMode=yes','-o','ControlMaster=auto','-o','ControlPersist=120','-o','ControlPath=/home/j/.ssh/gaia-publish-%C'];
const env={...process.env,GAIA_ARCHIVE_ROOT:root,GAIA_DB_PATH:root+'/gaia.sqlite3',GAIA_SOURCES:root+'/code/sources',GAIA_PUBLISH_LOOKBACK_SECONDS:'86400',GAIA_PREPROCESS_WORKERS:'16'};delete env.GAIA_PUBLISH_ALL;
const published=[];
for(const item of days){const day=item.date;if(!/^\d{4}-\d{2}-\d{2}$/.test(day))throw Error('Invalid date');const [year,month,date]=day.split('-'),folder=year+'-'+date+'-'+month;
 console.log('Publishing archive day',day,item.images,'stored images');
 for(const audience of ['public','open']){
  const local=root+'/'+audience,flavor=audience==='public'?'full':'open',remote='/mnt/shovel/gaia/'+folder+'/'+flavor;
  run(bin+'/gaia-server',['--publish'],{env:{...env,GAIA_PUBLISH_DATE:day,GAIA_PUBLISH_AUDIENCE:audience==='open'?'anonymous':'full'}});
  const raw=local+'/day-'+day+'.json';run(bin+'/gaia-overview',[raw]);
  const stage=fs.mkdtempSync(root+'/.day-'+day+'-');const manifest=stage+'/manifest.json';run('node',['deploy/overview-viewer.cjs',raw,manifest]);
  const names=run('node',['deploy/layer-assets.cjs',manifest,local+'/assets',audience],{capture:true});fs.writeFileSync(stage+'/assets.txt',names);
  run('ssh',[...ssh,'j@juha.no','mkdir -p '+remote+'/assets']);
  run('rsync',['-r','--size-only','--files-from='+stage+'/assets.txt','-e','ssh '+ssh.join(' '),local+'/assets/','j@juha.no:'+remote+'/assets/']);
  run('rsync',['-r','--checksum','-e','ssh '+ssh.join(' '),manifest,'j@juha.no:'+remote+'/manifest.pending']);
  run('rsync',['-r','-e','ssh '+ssh.join(' '),stage+'/assets.txt','j@juha.no:'+remote+'/assets.txt']);
  // Bounded nonempty checks before making the manifest visible; no shell evaluation of asset names.
  run('ssh',[...ssh,'j@juha.no',"cd "+remote+" && while IFS= read -r f; do test -s assets/\"$f\" || exit 1; done < assets.txt && mv manifest.pending manifest.json"]);
  if(audience==='public'){const dest=local+'/days/'+day;fs.mkdirSync(dest,{recursive:true});fs.copyFileSync(manifest,dest+'/manifest.pending');fs.renameSync(dest+'/manifest.pending',dest+'/manifest.json')}
 }
 published.push(item);
 const catalogue=JSON.stringify({days:published,generated_utc:new Date().toISOString()});
 fs.writeFileSync(root+'/public/days.json.pending',catalogue);fs.renameSync(root+'/public/days.json.pending',root+'/public/days.json');
 run('rsync',['-r','--checksum','-e','ssh '+ssh.join(' '),root+'/public/days.json','j@juha.no:/mnt/shovel/gaia/archive-days.pending']);
 run('ssh',[...ssh,'j@juha.no','mv /mnt/shovel/gaia/archive-days.pending /mnt/shovel/gaia/archive-days.json']);
 console.log('Verified and advertised archive day',day);
}
