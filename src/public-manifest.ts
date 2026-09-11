export const archiveMode = new URLSearchParams(location.search).has('archive');
const publicBuild=import.meta.env.VITE_GAIA_PUBLIC==='1';
export let manifestUrl=(publicBuild?'/gaia/open/':'/gaia/public/')+(archiveMode?'archive-manifest.json':'manifest.json');
export function setPublicAudience(authorized:boolean){if(publicBuild)manifestUrl=(authorized?'/gaia/restricted/':'/gaia/open/')+(archiveMode?'archive-manifest.json':'manifest.json');}
