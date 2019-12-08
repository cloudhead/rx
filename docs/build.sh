#!/bin/sh

sed '/<!-- # -->/{
  s/<!-- # -->//
  r /dev/stdin
}' "$1" | tr -s "\n"
