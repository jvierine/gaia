const query=new URLSearchParams(location.search);
const publicBuild=import.meta.env.VITE_GAIA_PUBLIC==='1';
export const selectedDay=/^\d{4}-\d{2}-\d{2}$/.test(query.get('date')||'')?query.get('date'):null;
export let canBrowseArchive=!publicBuild;
export let archiveMode=!publicBuild&&(!!selectedDay||query.has('archive'));
export let manifestUrl=selectedDay&&!publicBuild?'/gaia/public/days/'+selectedDay+'/manifest.json':publicBuild?'/gaia/open/manifest.json':'/gaia/public/manifest.json';
export const archiveDaysUrl=()=>publicBuild?'/gaia/history/days.json':'/gaia/public/days.json';
export function setPublicAudience(authorized:boolean,signedIn=false){if(publicBuild){canBrowseArchive=signedIn;archiveMode=signedIn&&!!selectedDay;manifestUrl=archiveMode?'/gaia/history/'+selectedDay+'/manifest.json':(authorized?'/gaia/restricted/':'/gaia/open/')+'manifest.json'}}
