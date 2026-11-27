#!/bin/sh
# Points the PKGBUILD beside it at a release: `update.sh <version> <deb sha256> <LICENSE sha256>`.
set -eu
version=$1
deb=$2
license=$3
pkgbuild="$(dirname "$0")/PKGBUILD"

sed -i \
  -e "s/^pkgver=.*/pkgver=$version/" \
  -e "s/^pkgrel=.*/pkgrel=1/" \
  -e "s/^sha256sums=('[0-9a-f]*'$/sha256sums=('$deb'/" \
  -e "s/^            '[0-9a-f]*')$/            '$license')/" \
  "$pkgbuild"
grep -q "^pkgver=$version\$" "$pkgbuild"
grep -q "'$deb'" "$pkgbuild"
grep -q "'$license')" "$pkgbuild"
