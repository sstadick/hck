#!/usr/bin/bash
set -eo pipefail

test_data="$1"
cp "${test_data}" ./hyper_data.txt

# Single Character Delimiter tests

# hyperfine --warmup 3 -m 5 --export-markdown single_char.md --show-output \
#     "xsv select -d, 1,15,18 --no-headers ./hyper_data.txt > /dev/null" \
#     'gcut -d, -f1,15,18 ./hyper_data.txt > /dev/null' \
#     'hck -d, -f1,15,18 ./hyper_data.txt > /dev/null' \
#     "gawk -F, '{print \$1, \$15, \$18}' ./hyper_data.txt > /dev/null"

# Multi character delimiter tests, create a file with three spaces as delmiter
cat ./hyper_data.txt |  hck -d, -f1- -D '   ' -o ./hyper_data_multichar.txt

# Notes:
#   - xsv gets messed up by the first commented line in the section, so tail skips it, the point is to run these commands as we would in the wild
hyperfine --warmup 3 -m 5 --export-markdown multi_char.md --show-output \
    "hck -d'\s+' -f1,15,18 ./hyper_data_multichar.txt > /dev/null" \
    "gawk -F' ' '{print \$1, \$15, \$18}' ./hyper_data_multichar.txt > /dev/null" \
    # "gawk -F'[:space:]+' '{print \$1, \$15, \$18}' ./hyper_data_multichar.txt > /dev/null" \
    # "< ./hyper_data_multichar.txt tr -s ' ' | tail -n+2 | xsv select -d ' ' 1,15,18 --no-headers > /dev/null" \
    # "< ./hyper_data_multichar.txt tr -s ' ' | gcut -d ' ' -f1,15,18 > /dev/null"


# rm ./hyper_data.txt
# rm ./hyper_data_multichar.txt
