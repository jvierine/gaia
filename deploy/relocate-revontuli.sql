-- Run only on the destination snapshot, before its service starts.
BEGIN IMMEDIATE;
UPDATE images SET archive_path='/mnt/data/juha/gaia/' || substr(archive_path,18)
WHERE archive_path LIKE '/mnt/shovel/gaia/%';
UPDATE calibrations SET hdf5_path='/mnt/data/juha/gaia/' || substr(hdf5_path,18)
WHERE hdf5_path LIKE '/mnt/shovel/gaia/%';
UPDATE mosaics SET hdf5_path='/mnt/data/juha/gaia/' || substr(hdf5_path,18)
WHERE hdf5_path LIKE '/mnt/shovel/gaia/%';
UPDATE mosaics SET preview_path='/mnt/data/juha/gaia/' || substr(preview_path,18)
WHERE preview_path LIKE '/mnt/shovel/gaia/%';
COMMIT;
PRAGMA quick_check;
