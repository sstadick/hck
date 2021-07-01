# 🪓 hck

<p align="center">
  <a href="https://github.com/sstadick/hck/actions?query=workflow%3ACheck"><img src="https://github.com/sstadick/hck/workflows/Check/badge.svg" alt="Build Status"></a>
  <img src="https://img.shields.io/crates/l/hck.svg" alt="license">
  <a href="https://crates.io/crates/hck"><img src="https://img.shields.io/crates/v/hck.svg?colorB=319e8c" alt="Version info"></a><br>
  A sharp <i>cut(1)</i> clone.
</p>

_`hck` is a shortening of `hack`, a rougher form of `cut`._

A close to drop in replacement for cut that can use a regex delimiter instead of a fixed string.
Additionally this tool allows for specification of the order of the output columns using the same column selection syntax as cut (see below for examples).

No single feature of `hck` on its own makes it stand out over `awk`, `cut`, `xsv` or other such tools. Where `hck` excels is making common things easy, such as reordering output fields, or splitting records on a weird delimiter.
It is meant to be simple and easy to use while exploring datasets.

## Features

- Reordering of output columns! i.e. if you use `-f4,2,8` the output columns will appear in the order `4`, `2`, `8`
- Delimiter treated as a regex (with `-R`), i.e. you can split on multiple spaces without and extra pipe to `tr`!
- Specification of output delimiter
- Selection of columns by header string literal with the `-F` option, or by regex by setting the `-r` flag
- Input files will be automatically decompressed if their file extension is recognizable and a local binary exists to perform the decompression (similar to ripgrep)

## Install

With the Rust toolchain:

```bash
cargo install hck
```

From the [releases page](https://github.com/sstadick/hck/releases)

## Examples

### Splitting with a regex delimiter

```bash
ps aux | hck -d'\s+' -R -f1-3,5-
```

### Reordering output columns

```bash
ps aux | hck -d'\s+' -R -f2,1,3-
```

### Changing the output record separator

```bash
ps aux | hck -d'\s+' -R -D'___' -f2,1,3-
```

### Select columns with regex

```bash
hck -F 'is_new.*` -F'^[^_]' -r ./headered_data.tsv
```

### Automagic decompresion

```bash
hck -f1,3- -z ~/Downloads/massive.tsv.gz | rg 'cool_data'
```

## Benchmarks

This set of benchmarks is simply meant to show that `hck` is in the same ballpark as other tools. These are meant to capture real world usage of the tools, so in the multi-space delimiter benchmark for `gcut`, for example, we use `tr` to convert the space runs to a single space and then pipe to `gcut`.

*Note* this is not meant to be an authoritative set of benchmarks, it is just meant to give a relative sense of performance of different ways of accomplishing the same tasks.

#### Hardware

Ubuntu 20 AMD Ryzen 9 3950X 16-Core Processor w/ 64 GB DDR4 memory and 1TB NVMe Drive

#### Data

The [all_train.csv](https://archive.ics.uci.edu/ml/machine-learning-databases/00347/all_train.csv.gz) data is used.

This is a CSV dataset with 7 million lines. We test it both using `,` as the delimiter, and then also using `\s\s\s` as a delimiter.

PRs are welcome for benchmarks with more tools, or improved (but still realistic) pipelines for commands.

#### Tools

`cut`:
  - https://www.gnu.org/software/coreutils/manual/html_node/The-cut-command.html
  - 8.30

`mawk`:
  - https://invisible-island.net/mawk/mawk.html
  - v1.3.4

`xsv`:
  - https://github.com/BurntSushi/xsv
  - v0.13.0

`tsv-utils`:
  - https://github.com/eBay/tsv-utils
  - v2.2.0 (ldc2)

### Single character delimiter benchmark

| Command                                                      |      Mean [s] | Min [s] | Max [s] |    Relative |
| :----------------------------------------------------------- | ------------: | ------: | ------: | ----------: |
| `hck -d, -f1,8,19 ./hyper_data.txt > /dev/null`              | 1.800 ± 0.024 |   1.775 |   1.829 |        1.00 |
| `tsv-select -d, -f 1,8,19 ./hyper_data.txt > /dev/null`      | 1.831 ± 0.002 |   1.828 |   1.834 | 1.02 ± 0.01 |
| `xsv select -d, 1,8,19 ./hyper_data.txt > /dev/null`         | 5.623 ± 0.010 |   5.613 |   5.641 | 3.12 ± 0.04 |
| `awk -F, '{print $1, $8, $19}' ./hyper_data.txt > /dev/null` | 4.979 ± 0.086 |   4.901 |   5.127 | 2.77 ± 0.06 |
| `cut -d, -f1,8,19 ./hyper_data.txt > /dev/null`              | 6.883 ± 0.082 |   6.822 |   7.019 | 3.82 ± 0.07 |


### Multi-character delimiter benchmark

| Command                                                                                                    |       Mean [s] | Min [s] | Max [s] |    Relative |
| :--------------------------------------------------------------------------------------------------------- | -------------: | ------: | ------: | ----------: |
| `hck -d'   ' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                              |  2.729 ± 0.020 |   2.706 |   2.751 |        1.00 |
| `hck -d'\s+' -f1,8,19 -R ./hyper_data_multichar.txt > /dev/null`                                           | 12.357 ± 0.006 |  12.348 |  12.363 | 4.53 ± 0.03 |
| `awk -F' ' '{print $1, $8 $19}' ./hyper_data_multichar.txt > /dev/null`                                    |  6.789 ± 0.032 |   6.759 |   6.839 | 2.49 ± 0.02 |
| `awk -F'   ' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                                 |  5.850 ± 0.153 |   5.650 |   5.981 | 2.14 ± 0.06 |
| `awk -F'[:space:]+' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                          | 10.831 ± 0.120 |  10.644 |  10.959 | 3.97 ± 0.05 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| cut -d ' ' -f1,8,19 > /dev/null`                                |  7.493 ± 0.081 |   7.425 |   7.625 | 2.75 ± 0.04 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tail -n+2 \| xsv select -d ' ' 1,8,19 --no-headers > /dev/null` |  6.840 ± 0.101 |   6.663 |   6.912 | 2.51 ± 0.04 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| hck -d' ' -f1,8,19 > /dev/null`                                 |  6.290 ± 0.036 |   6.254 |   6.341 | 2.30 ± 0.02 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tsv-select -d ' ' -f 1,8,19 > /dev/null`                        |  6.209 ± 0.150 |   5.964 |   6.351 | 2.27 ± 0.06 |

## TODO

- Add complement argument
- Don't reparse fields / headers for each new file
- Look at ripgreps searcher crate and how it iterates over lines
- Bake in grep somehow?
- Move tests from main to core

## Questions

I've ripped the code out of the bstr line closure to go faster. The lifetime coercion on the cached vec `shuffler` makes it really hard to break that function because as soon as we start to store things on structs the the compiler realizes what we're doing and throws a fit. Additinally, I haven't found a good way to be generic over an iterator produced by split on regex vs split on bstr. I think the solution might be wrapping in a concrete type but I'm not sure. Overally I'd love for someone who really knows what they are doing to see if they can:

- Fix up the `line_parser.rs` code so that a concrete `LineParser` object can be passed to the `Core` and used to parse lines.
- Work out a better way to reuse the `shuffler` vec, or not use it altogether.

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
