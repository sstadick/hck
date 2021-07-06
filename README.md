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
| `hck -d, -f1,8,19 ./hyper_data.txt > /dev/null`              | 1.461 ± 0.003 |   1.458 |   1.465 |        1.00 |
| `hck -d, -f1,8,19 --no-mmap ./hyper_data.txt > /dev/null`    | 1.643 ± 0.003 |   1.641 |   1.648 | 1.12 ± 0.00 |
| `tsv-select -d, -f 1,8,19 ./hyper_data.txt > /dev/null`      | 1.759 ± 0.009 |   1.748 |   1.770 | 1.20 ± 0.01 |
| `xsv select -d, 1,8,19 ./hyper_data.txt > /dev/null`         | 5.671 ± 0.069 |   5.631 |   5.794 | 3.88 ± 0.05 |
| `awk -F, '{print $1, $8, $19}' ./hyper_data.txt > /dev/null` | 4.928 ± 0.059 |   4.864 |   4.980 | 3.37 ± 0.04 |
| `cut -d, -f1,8,19 ./hyper_data.txt > /dev/null`              | 6.869 ± 0.008 |   6.861 |   6.883 | 4.70 ± 0.01 |


### Multi-character delimiter benchmark

| Command                                                                                                    |       Mean [s] | Min [s] | Max [s] |    Relative |
| :--------------------------------------------------------------------------------------------------------- | -------------: | ------: | ------: | ----------: |
| `hck -d'   ' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                              |  1.934 ± 0.011 |   1.919 |   1.947 |        1.00 |
| `hck -d'   ' -f1,8,19 --no-mmap ./hyper_data_multichar.txt > /dev/null`                                    |  2.197 ± 0.006 |   2.188 |   2.202 | 1.14 ± 0.01 |
| `hck -d'\s+' -f1,8,19 -R ./hyper_data_multichar.txt > /dev/null`                                           | 10.503 ± 0.060 |  10.451 |  10.596 | 5.43 ± 0.04 |
| `hck -d'\s+' -f1,8,19 --no-mmap -R ./hyper_data_multichar.txt > /dev/null`                                 | 10.930 ± 0.093 |  10.769 |  11.006 | 5.65 ± 0.06 |
| `awk -F'   ' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                                 |  5.853 ± 0.069 |   5.799 |   5.970 | 3.03 ± 0.04 |
| `awk -F' ' '{print $1, $8 $19}' ./hyper_data_multichar.txt > /dev/null`                                    |  6.693 ± 0.108 |   6.593 |   6.863 | 3.46 ± 0.06 |
| `awk -F'[:space:]+' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                          | 10.593 ± 0.170 |  10.429 |  10.846 | 5.48 ± 0.09 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| hck -d' ' -f1,8,19 > /dev/null`                                 |  6.098 ± 0.033 |   6.068 |   6.141 | 3.15 ± 0.02 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tsv-select -d ' ' -f 1,8,19 > /dev/null`                        |  6.130 ± 0.090 |   6.058 |   6.229 | 3.17 ± 0.05 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tail -n+2 \| xsv select -d ' ' 1,8,19 --no-headers > /dev/null` |  6.551 ± 0.182 |   6.238 |   6.710 | 3.39 ± 0.10 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| cut -d ' ' -f1,8,19 > /dev/null`                                |  7.273 ± 0.065 |   7.222 |   7.384 | 3.76 ± 0.04 |

## TODO

- Add complement argument
- Don't reparse fields / headers for each new file
- Allow for two runmodes - buffered or mmap, configure similar to how ripgrep does it (care for the -z option as well)
- Verify that ripgrep reallydoes use mmap pretty much always
- Add a directio mode similar to mmap mode?
- Add a heuristic if using single byte delimiters to use memchr2 to look for newlines and delims at the same time and 'annotate' the line accordingly. Should save one pass over the data.
- Bake in grep somehow?
- Move tests from main to core
- Add more tests all around
- Think about adding a full on sync mode

## Questions

I've ripped the code out of the bstr line closure to go faster. The lifetime coercion on the cached vec `shuffler` makes it really hard to break that function because as soon as we start to store things on structs the the compiler realizes what we're doing and throws a fit. Additinally, I haven't found a good way to be generic over an iterator produced by split on regex vs split on bstr. I think the solution might be wrapping in a concrete type but I'm not sure. Overally I'd love for someone who really knows what they are doing to see if they can:

- Fix up the `line_parser.rs` code so that a concrete `LineParser` object can be passed to the `Core` and used to parse lines.
- Work out a better way to reuse the `shuffler` vec, or not use it altogether.

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
