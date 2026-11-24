#!/usr/bin/env bash
# Status line and hook command for the fake accounts: keeps each raw input in order, prints nothing.
dir="$(dirname "$0")/recorded"
mkdir -p "$dir"
stamp=$(date +%s%N)
cat > "$dir/$stamp-$1.json"
