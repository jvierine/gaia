export const archiveMode = new URLSearchParams(location.search).has('archive');
export const manifestUrl = archiveMode ? '/gaia/public/archive-manifest.json' : '/gaia/public/manifest.json';
