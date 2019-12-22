#!/bin/sh

sed '/<!-- {page} -->/{
  s/<!-- {page} -->//
  r /dev/stdin
}' "$2" | sed "s/{id}/$1/g"
