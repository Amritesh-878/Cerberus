#!/bin/sh
# Builds the package in this folder as the `builder` user, checks what it installs, and fails on namcap errors.
set -eu

# Its runtime libraries are large and not needed to pack and lint it.
sudo -u builder makepkg --nodeps --noconfirm --cleanbuild
pkg=$(ls ./*.pkg.tar.zst)
listed=$(bsdtar -tf "$pkg")
for file in usr/bin/cerberus usr/bin/cerberus-cli usr/bin/cerberus-mcp usr/bin/cerberus-live \
  usr/share/applications/Cerberus.desktop usr/share/licenses/cerberus-bin/LICENSE; do
  echo "$listed" | grep -qx "$file" || { echo "$file is missing from $pkg"; exit 1; }
done

namcap PKGBUILD | tee namcap.txt
namcap "$pkg" | tee -a namcap.txt
if grep -q ' E: ' namcap.txt; then
  echo "namcap reported errors"
  exit 1
fi
