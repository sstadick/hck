#!/usr/bin/bash
set -eo pipefail

test_data="$1"
# cp "${test_data}" ./hyper_data.txt

# Single Character Delimiter tests

hyperfine --warmup 3 -m 5 --export-markdown single_char.md --show-output \
    'hck -d, -f1,8,19 ./hyper_data.txt > /dev/null' \
    'hck -d, -f1,8,19 --no-mmap ./hyper_data.txt > /dev/null' \
    "tsv-select -d, -f 1,8,19 ./hyper_data.txt > /dev/null" \
    "xsv select -d, 1,8,19 ./hyper_data.txt > /dev/null" \
    "awk -F, '{print \$1, \$8, \$19}' ./hyper_data.txt > /dev/null" \
    'cut -d, -f1,8,19 ./hyper_data.txt > /dev/null'

# Multi character delimiter tests, create a file with three spaces as delmiter
hck -d, -f1- -D '   ' -o ./hyper_data_multichar.txt ./hyper_data.txt

# Notes:
#   - xsv gets messed up by the first commented line in the section, so tail skips it, the point is to run these commands as we would in the wild
hyperfine --warmup 3 -m 5 --export-markdown multi_char.md --show-output \
    "hck -d'\s+' -f1,8,19 -R ./hyper_data_multichar.txt > /dev/null" \
    "hck -d'   ' -f1,8,19 ./hyper_data_multichar.txt > /dev/null" \
    "hck -d'\s+' -f1,8,19 --no-mmap -R ./hyper_data_multichar.txt > /dev/null" \
    "hck -d'   ' -f1,8,19 --no-mmap ./hyper_data_multichar.txt > /dev/null" \
    "awk -F' ' '{print \$1, \$8 \$19}' ./hyper_data_multichar.txt > /dev/null" \
    "awk -F'   ' '{print \$1, \$8, \$19}' ./hyper_data_multichar.txt > /dev/null" \
    "awk -F'[:space:]+' '{print \$1, \$8, \$19}' ./hyper_data_multichar.txt > /dev/null" \
    "< ./hyper_data_multichar.txt tr -s ' ' | cut -d ' ' -f1,8,19 > /dev/null" \
    "< ./hyper_data_multichar.txt tr -s ' ' | tail -n+2 | xsv select -d ' ' 1,8,19 --no-headers > /dev/null" \
    "< ./hyper_data_multichar.txt tr -s ' ' | hck -d' ' -f1,8,19 > /dev/null" \
    "< ./hyper_data_multichar.txt tr -s ' ' | tsv-select -d ' ' -f 1,8,19 > /dev/null"


# rm ./hyper_data.txt
# rm ./hyper_data_multichar.txt
