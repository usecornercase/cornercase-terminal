#!/bin/bash
file=${1:-src/returns/address.rs}
rows=$(tput lines)
printf '\033[2J\033[H'
n=0
while IFS= read -r line || [ -n "$line" ]; do
  n=$((n + 1))
  printf '\033[2m%3d\033[0m %s\n' "$n" "$line"
done < "$file"
while [ "$n" -lt $((rows - 2)) ]; do
  n=$((n + 1))
  printf '\033[34m~\033[0m\n'
done
printf '\033[7m %-40s %18s \033[0m\n' "$file" '11,1          All'
printf '\033[12;5H'
while IFS= read -r -n1 _; do :; done
