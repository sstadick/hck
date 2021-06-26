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
- Selection of columns by header regex with the `-F` option, or by string literal by setting the `-L` flag
- Input files will be automatically decompressed if their file extension is recognizable and a local binary exists to perform the decompression

## Install

With the Rust toolchain:

```bash
cargo install hck
```

From the releases page:

```bash
wget ...
```

## Examples

### Splitting with a regex delimiter

### Reordering output columns

### Changing the output record separator

### Select columns with regex

### Automagic decompresion

## Benchmarks

This set of benchmarks is simply meant to show that `hck` is in the same ballpark as other tools. These are meant to capture real world usage of the tools, so in the multi-space delimiter benchmark for `gcut`, for example, we use `tr` to convert the space runs to a single space and then pipe to `gcut`.

#### Hardware

MacBook Pro 2.3 GHz 8-Core Intel i9 w/ 32 GB 2667 MHz DDR4 memory and 1TB Flash Storage

#### Data

The [all_train.csv](https://archive.ics.uci.edu/ml/machine-learning-databases/00347/all_train.csv.gz) data is used.

This is a CSV dataset with 7 million lines. We test it both using `,` as the delimiter, and then also using `\s\s\s` as a delimiter.

PRs are welcome for benchmarks with more tools, or improved (but still realistic) pipelines for commands.

#### Tools

`gcut`: cut from coreutils installed via brew

`gawk`: https://www.gnu.org/software/gawk/

`xsv`: https://github.com/BurntSushi/xsv

TODO: add verions
TODO: add other tools?
TODO: Install xsv and hck from binaries rather than compiling them?

### Single character delimiter benchmark

| Command                                                            |       Mean [s] | Min [s] | Max [s] |    Relative |
| :----------------------------------------------------------------- | -------------: | ------: | ------: | ----------: |
| `xsv select -d, 1,15,18 --no-headers ./hyper_data.txt > /dev/null` |  7.059 ± 0.324 |   6.479 |   7.211 | 1.57 ± 0.12 |
| `gcut -d, -f1,15,18 ./hyper_data.txt > /dev/null`                  | 11.016 ± 0.088 |  10.869 |  11.109 | 2.45 ± 0.15 |
| `hck -d, -f1,15,18 -i ./hyper_data.txt > /dev/null`                |  4.500 ± 0.278 |   4.009 |   4.657 |        1.00 |
| `gawk -F, '{print $1, $15, $18}' ./hyper_data.txt > /dev/null`     | 28.290 ± 0.392 |  27.764 |  28.643 | 6.29 ± 0.40 |


### Regex delimiter benchmark

| Command                                                                                                     |       Mean [s] | Min [s] | Max [s] |     Relative |
| :---------------------------------------------------------------------------------------------------------- | -------------: | ------: | ------: | -----------: |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tail -n+2 \| xsv select -d ' ' 1,15,18 --no-headers > /dev/null` | 74.005 ± 2.214 |  70.811 |  75.952 | 41.80 ± 1.25 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| gcut -d ' ' -f1,15,18 > /dev/null`                               | 67.295 ± 0.791 |  66.348 |  68.243 | 38.01 ± 0.46 |
| `hck -d'\s+' -f1,15,18 -i ./hyper_data_multichar.txt > /dev/null`                                           |  2.171 ± 0.121 |   1.957 |   2.246 |  1.23 ± 0.07 |
| `gawk -F' ' '{print $1, $15, $18}' ./hyper_data_multichar.txt > /dev/null`                                  |  1.770 ± 0.004 |   1.767 |   1.775 |         1.00 |


### Selecting columns with headers

## TODO

- Add complement argument

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
