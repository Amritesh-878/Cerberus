#!/bin/bash
# Builds the package in this folder as the `builder` user, checks what it installs, and fails on namcap errors.
set -eu

# namcap judges the libraries the binaries need against what is installed, so the dependencies go in first.
# shellcheck source=/dev/null
. ./PKGBUILD
pacman -S --noconfirm --needed "${depends[@]}"
sudo -u builder makepkg --noconfirm --cleanbuild
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
