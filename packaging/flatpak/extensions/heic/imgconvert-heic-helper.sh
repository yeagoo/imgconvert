#!/bin/sh
# SPDX-License-Identifier: Apache-2.0

set -eu

if [ "$#" -lt 2 ] || [ "$#" -gt 3 ]; then
  echo "usage: imgconvert-heic-helper INPUT OUTPUT [METADATA_JSON]" >&2
  exit 64
fi

input=$1
output=$2
metadata=${3:-}

case "$output" in
  *.png | *.PNG) ;;
  *)
    echo "output path must end with .png" >&2
    exit 65
    ;;
esac

heif-convert "$input" "$output" >/dev/null

if [ -n "$metadata" ]; then
  printf '{"version":1}\n' > "$metadata"
fi
