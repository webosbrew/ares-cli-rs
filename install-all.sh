#!/bin/sh

for f in ares-*; do
  cargo install --path "$f"
done