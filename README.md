# 🪓 hck

<p align="center">
  <a href="https://github.com/sstadick/hck/actions?query=workflow%3ACheck"><img src="https://github.com/sstadick/hck/workflows/Check/badge.svg" alt="Build Status"></a>
  <img src="https://img.shields.io/crates/l/hck.svg" alt="license">
  <a href="https://crates.io/crates/hck"><img src="https://img.shields.io/crates/v/hck.svg?colorB=319e8c" alt="Version info"></a><br>
  A sharp <i>cut(1)</i> clone.
</p>

_`hck` is a shortening of `hack`, a rougher form of `cut`._

A close to drop in replacement for cut that uses a regex delimiter instead of a fixed string.
Additionally this tool allows for specification of the order of the output columns using the same column selection syntax as cut (see below for examples).

No single feature of `hck` on its own makes it stand out over `awk`, `cut`, `xsv` or other such tools. Where `hck` excels is making common things easy, such as reordering output fields, or splitting records on a weird delimiter.
It is meant to be simple and easy to use while exploring datasets.

## Features

- Reordering of output columns! i.e. if you use `-f4,2,8` the output columns will appear in the order `4`, `2`, `8`
- Regex delimiter, i.e. you can split on multiple spaces without and extra pipe to `tr`!
- Specification of output delimiter
- Selection of columns by header regex with the `-F` option, or by string literal by setting the `-L` flag
- Input files will be automatically decompressed if their file extension is recognizable and a local binary exists to perform the decompression (similar to ripgrep)

## Install

With the Rust toolchain:

```bash
# Note, you may have to specify the version with the beta releases
cargo install hck
```

From the releases page:

```bash
wget ...
```

## Examples

TODO: maybe use ps aux for all these

### Splitting with a regex delimiter

### Reordering output columns

### Changing the output record separator

### Select columns with regex

TODO: Use `ps aux` as example of weirdly spaced thing you may want to parse as tabular

### Automagic decompresion

## Benchmarks

This set of benchmarks is simply meant to show that `hck` is in the same ballpark as other tools. These are meant to capture real world usage of the tools, so in the multi-space delimiter benchmark for `gcut`, for example, we use `tr` to convert the space runs to a single space and then pipe to `gcut`.

*Note* this is not meant to be an authoritative set of benchmarks, it is just meant to give a relative sense of performance of different ways of accomplishing the same tasks.

#### Hardware

MacBook Pro 2.3 GHz 8-Core Intel i9 w/ 32 GB 2667 MHz DDR4 memory and 1TB Flash Storage

#### Data

The [all_train.csv](https://archive.ics.uci.edu/ml/machine-learning-databases/00347/all_train.csv.gz) data is used.

This is a CSV dataset with 7 million lines. We test it both using `,` as the delimiter, and then also using `\s\s\s` as a delimiter.

PRs are welcome for benchmarks with more tools, or improved (but still realistic) pipelines for commands.

#### Tools

`gcut`:
  - https://www.gnu.org/software/coreutils/manual/html_node/The-cut-command.html
  - 8.32

`gawk`:
  - https://www.gnu.org/software/gawk/
  - v5.1.0

`xsv`:
  - https://github.com/BurntSushi/xsv
  - v0.13.0

`tsv-utils`:
  - https://github.com/eBay/tsv-utils
  - v2.2.0 (ldc2)

### Single character delimiter benchmark

| Command                                                           |       Mean [s] | Min [s] | Max [s] |    Relative |
| :---------------------------------------------------------------- | -------------: | ------: | ------: | ----------: |
| `hck -d, -f1,8,19 ./hyper_data.txt > /dev/null`                   |  4.017 ± 0.019 |   4.001 |   4.048 |        1.00 |
| `gawk -F, '{print $1, $8, $19}' ./hyper_data.txt > /dev/null`     | 26.765 ± 0.187 |  26.600 |  26.980 | 6.66 ± 0.06 |
| `gcut -d, -f1,8,19 ./hyper_data.txt > /dev/null`                  | 10.835 ± 0.159 |  10.608 |  11.036 | 2.70 ± 0.04 |
| `xsv select -d, 1,8,19 --no-headers ./hyper_data.txt > /dev/null` |  6.833 ± 0.091 |   6.732 |   6.923 | 1.70 ± 0.02 |
| `tsv-select -f 1,8,19 --no-headers ./hyper_data.txt > /dev/null`  |  6.833 ± 0.091 |   6.732 |   6.923 | 1.70 ± 0.02 |


### Regex delimiter benchmark

| Command                                                                                                    |        Mean [s] | Min [s] | Max [s] |     Relative |
| :--------------------------------------------------------------------------------------------------------- | --------------: | ------: | ------: | -----------: |
| `hck -d'\s+' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                              |  14.854 ± 0.223 |  14.547 |  15.139 |  2.70 ± 0.06 |
| `hck -d'   ' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                              |   5.506 ± 0.099 |   5.354 |   5.630 |         1.00 |
| `gawk -F' ' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                                  |  10.933 ± 0.079 |  10.832 |  11.049 |  1.99 ± 0.04 |
| `gawk -F'   ' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                                |  30.225 ± 0.324 |  29.875 |  30.757 |  5.49 ± 0.12 |
| `gawk -F'[:space:]+' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                         |  29.373 ± 0.360 |  28.942 |  29.733 |  5.33 ± 0.12 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| gcut -d ' ' -f1,8,19 > /dev/null`                               | 439.325 ± 1.180 | 438.133 | 441.258 | 79.79 ± 1.45 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tail -n+2 \| xsv select -d ' ' 1,8,19 --no-headers > /dev/null` | 453.706 ± 1.065 | 452.155 | 454.765 | 82.40 ± 1.50 |

## TODO

- Add complement argument

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
