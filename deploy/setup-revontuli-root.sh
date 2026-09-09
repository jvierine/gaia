#!/bin/sh
set -eu
# Run on Revontuli with sudo after reviewing apache-revontuli.conf.
apt-get update
apt-get install -y tesseract-ocr tesseract-ocr-eng nodejs sqlite3
install -m 644 /mnt/data/juha/gaia/code/deploy/apache-revontuli.conf /etc/apache2/conf-available/gaia-standalone.conf
a2enmod proxy proxy_http alias
a2enconf gaia-standalone
apache2ctl configtest
systemctl reload apache2
