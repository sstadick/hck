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
- Speed

## Install

With the Rust toolchain:

```bash
export RUSTFLAGS='-C target-cpu=native'
cargo install hck
```

From the [releases page](https://github.com/sstadick/hck/releases)

## Examples

### Splitting with a string literal

```bash
❯ hck -Ld' ' -f1-3,5- ./README.md | head -n4
#       🪓      hck

<p      align="center">
                <a      src="https://github.com/sstadick/hck/workflows/Check/badge.svg" alt="Build      Status"></a>
```

### Splitting with a regex delimiter

```bash
# note, '\s+' is the default
❯ ps aux | hck -f1-3,5- | head -n4
USER    PID     %CPU    VSZ     RSS     TTY     STAT    START   TIME    COMMAND
root    1       0.0     169452  13472   ?       Ss      Jun21   0:19    /sbin/init      splash
root    2       0.0     0       0       ?       S       Jun21   0:00    [kthreadd]
root    3       0.0     0       0       ?       I<      Jun21   0:00    [rcu_gp]
```

### Reordering output columns

```bash
❯ ps aux | hck -f2,1,3- | head -n4
PID     USER    %CPU    %MEM    VSZ     RSS     TTY     STAT    START   TIME    COMMAND
1       root    0.0     0.0     169452  13472   ?       Ss      Jun21   0:19    /sbin/init      splash
2       root    0.0     0.0     0       0       ?       S       Jun21   0:00    [kthreadd]
3       root    0.0     0.0     0       0       ?       I<      Jun21   0:00    [rcu_gp]
```

### Changing the output record separator

```bash
❯ ps aux | hck -D'___' -f2,1,3 | head -n4
PID___USER___%CPU
1___root___0.0
2___root___0.0
3___root___0.0
```

### Select columns with regex

```bash
# Note the order match the order of the -F args
ps aux | hck -r -F '^ST.*' -F '^USER$' | head -n4
STAT    START   USER
Ss      Jun21   root
S       Jun21   root
I<      Jun21   root
```

### Automagic decompresion

```bash
❯ gzip ./README.md
❯ hck -Ld' ' -f1-3,5- -z ./README.md.gz | head -n4
#       🪓      hck

<p      align="center">
                <a      src="https://github.com/sstadick/hck/workflows/Check/badge.svg" alt="Build      Status"></a>
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
  - v0.13.0 (compiled locally with optimations)

`tsv-utils`:
  - https://github.com/eBay/tsv-utils
  - v2.2.0 (ldc2, compiled locally with optimizations)

### Single character delimiter benchmark

| Command                                                      |      Mean [s] | Min [s] | Max [s] |    Relative |
| :----------------------------------------------------------- | ------------: | ------: | ------: | ----------: |
| `hck -Ld, -f1,8,19 ./hyper_data.txt > /dev/null`             | 1.590 ± 0.004 |   1.587 |   1.595 |        1.00 |
| `hck -Ld, -f1,8,19 --no-mmap ./hyper_data.txt > /dev/null`   | 1.674 ± 0.006 |   1.668 |   1.683 | 1.05 ± 0.00 |
| `tsv-select -d, -f 1,8,19 ./hyper_data.txt > /dev/null`      | 1.766 ± 0.004 |   1.760 |   1.770 | 1.11 ± 0.00 |
| `xsv select -d, 1,8,19 ./hyper_data.txt > /dev/null`         | 5.540 ± 0.083 |   5.477 |   5.635 | 3.48 ± 0.05 |
| `awk -F, '{print $1, $8, $19}' ./hyper_data.txt > /dev/null` | 5.034 ± 0.100 |   4.955 |   5.195 | 3.17 ± 0.06 |
| `cut -d, -f1,8,19 ./hyper_data.txt > /dev/null`              | 6.971 ± 0.566 |   6.693 |   7.983 | 4.38 ± 0.36 |

### Multi-character delimiter benchmark

| Command                                                                                                    |       Mean [s] | Min [s] | Max [s] |    Relative |
| :--------------------------------------------------------------------------------------------------------- | -------------: | ------: | ------: | ----------: |
| `hck -Ld'   ' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                             |  1.905 ± 0.013 |   1.885 |   1.921 |        1.00 |
| `hck -Ld'   ' -f1,8,19 --no-mmap ./hyper_data_multichar.txt > /dev/null`                                   |  2.203 ± 0.022 |   2.173 |   2.226 | 1.16 ± 0.01 |
| `hck -d'\s+' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                              | 11.314 ± 0.167 |  11.083 |  11.436 | 5.94 ± 0.10 |
| `hck -d'\s+' -f1,8,19 --no-mmap ./hyper_data_multichar.txt > /dev/null`                                    | 11.434 ± 0.010 |  11.427 |  11.449 | 6.00 ± 0.04 |
| `awk -F' ' '{print $1, $8 $19}' ./hyper_data_multichar.txt > /dev/null`                                    |  6.460 ± 0.007 |   6.450 |   6.470 | 3.39 ± 0.02 |
| `awk -F'   ' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                                 |  5.837 ± 0.086 |   5.738 |   5.920 | 3.06 ± 0.05 |
| `awk -F'[:space:]+' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                          | 10.700 ± 0.075 |  10.583 |  10.791 | 5.62 ± 0.06 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| cut -d ' ' -f1,8,19 > /dev/null`                                |  7.522 ± 0.137 |   7.333 |   7.709 | 3.95 ± 0.08 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tail -n+2 \| xsv select -d ' ' 1,8,19 --no-headers > /dev/null` |  6.866 ± 0.090 |   6.770 |   6.997 | 3.61 ± 0.05 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| hck -d' ' -f1,8,19 > /dev/null`                                 |  6.269 ± 0.124 |   6.060 |   6.386 | 3.29 ± 0.07 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tsv-select -d ' ' -f 1,8,19 > /dev/null`                        |  6.270 ± 0.145 |   6.037 |   6.396 | 3.29 ± 0.08 |

## TODO

- Add complement argument
- Support indexing from the end
- Don't reparse fields / headers for each new file
- figure out how to better reuse / share a vec
- Bake in grep / filtering somehow?
- Move tests from main to core
- Add more tests all around
- Add preprocessor / pigz support
- Implement parallel parser as described [here](https://www.semanticscholar.org/paper/Instant-Loading-for-Main-Memory-Databases-M%C3%BChlbauer-R%C3%B6diger/a1b067fc941d6727169ec18a882080fa1f074595?p2df) This should be very doable given we don't care about escaping quotes and such.

## Questions

I've ripped the code out of the bstr line closure to go faster. The lifetime coercion on the cached vec `shuffler` makes it really hard to break that function because as soon as we start to store things on structs the the compiler realizes what we're doing and throws a fit. Additinally, I haven't found a good way to be generic over an iterator produced by split on regex vs split on bstr. I think the solution might be wrapping in a concrete type but I'm not sure. Overally I'd love for someone who really knows what they are doing to see if they can:

- Fix up the `line_parser.rs` code so that a concrete `LineParser` object can be passed to the `Core` and used to parse lines.
- Work out a better way to reuse the `shuffler` vec, or not use it altogether.

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
- [ripgrep](https://github.com/BurntSushi/ripgrep/tree/master/crates/searcher)
