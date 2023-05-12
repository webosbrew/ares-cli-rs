#!/bin/sh

packages=$(cargo tree --depth=0 --prefix=none | sed -r 's/.+\((.+)\)/\1/g' | tr -s '\n')

for f in $packages; do
  cargo install --path "$f"
done