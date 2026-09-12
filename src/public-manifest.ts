const requestedArchive=new URLSearchParams(location.search).has('archive');
export let archiveMode=requestedArchive&&import.meta.env.VITE_GAIA_PUBLIC!=='1';
const publicBuild=import.meta.env.VITE_GAIA_PUBLIC==='1';
export let manifestUrl=(publicBuild?'/gaia/open/':'/gaia/public/')+(archiveMode?'archive-manifest.json':'manifest.json');
export function setPublicAudience(authorized:boolean){if(publicBuild){archiveMode=requestedArchive&&authorized;manifestUrl=(authorized?'/gaia/restricted/':'/gaia/open/')+(archiveMode?'archive-manifest.json':'manifest.json');}}
