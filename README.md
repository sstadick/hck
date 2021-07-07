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
- Input files will be automatically decompressed if their file extension is recognizable and a local binary exists to perform the decompression (similar to ripgrep). See [Decompression](#decompression).
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

### Splitting on multiple characters

```bash
# with string literal
❯ printf 'this$;$is$;$a$;$test\na$;$b$;$3$;$four\n' > test.txt
❯ hck -Ld'$;$' -f3,4 ./test.txt
a       test
3       four
# with an interesting regex
❯ printf 'this123__is456--a789-test\na129_-b849-_3109_-four\n' > test.txt
❯ hck -d'\d{3}[-_]+' -f3,4 ./test.txt
a       test
3       four
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
  - v0.13.0 (compiled locally with optimizations)

`tsv-utils`:
  - https://github.com/eBay/tsv-utils
  - v2.2.0 (ldc2, compiled locally with optimizations)

`choose`:
  - https://github.com/theryangeary/choose
  - v1.3.1 (compiled locally with optimizations)

### Single character delimiter benchmark

| Command                                                      |      Mean [s] | Min [s] | Max [s] |    Relative |
| :----------------------------------------------------------- | ------------: | ------: | ------: | ----------: |
| `hck -Ld, -f1,8,19 ./hyper_data.txt > /dev/null`             | 1.525 ± 0.012 |   1.513 |   1.544 |        1.00 |
| `hck -Ld, -f1,8,19 --no-mmap ./hyper_data.txt > /dev/null`   | 1.752 ± 0.006 |   1.745 |   1.762 | 1.15 ± 0.01 |
| `hck -d, -f1,8,19  ./hyper_data.txt > /dev/null`             | 2.246 ± 0.086 |   2.146 |   2.342 | 1.47 ± 0.06 |
| `hck -d, -f1,8,19  --no-mmap ./hyper_data.txt > /dev/null`   | 2.522 ± 0.011 |   2.511 |   2.539 | 1.65 ± 0.01 |
| `choose -f , -i ./hyper_data.txt 0 7 18  > /dev/null`        | 4.442 ± 0.006 |   4.435 |   4.451 | 2.91 ± 0.02 |
| `tsv-select -d, -f 1,8,19 ./hyper_data.txt > /dev/null`      | 1.767 ± 0.005 |   1.763 |   1.775 | 1.16 ± 0.01 |
| `xsv select -d, 1,8,19 ./hyper_data.txt > /dev/null`         | 5.746 ± 0.072 |   5.668 |   5.833 | 3.77 ± 0.06 |
| `awk -F, '{print $1, $8, $19}' ./hyper_data.txt > /dev/null` | 5.116 ± 0.057 |   5.067 |   5.214 | 3.35 ± 0.05 |
| `cut -d, -f1,8,19 ./hyper_data.txt > /dev/null`              | 7.387 ± 0.609 |   6.981 |   8.415 | 4.84 ± 0.40 |

### Multi-character delimiter benchmark

| Command                                                                                                    |       Mean [s] | Min [s] | Max [s] |     Relative |
| :--------------------------------------------------------------------------------------------------------- | -------------: | ------: | ------: | -----------: |
| `hck -Ld'   ' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                             |  2.035 ± 0.011 |   2.026 |   2.053 |         1.00 |
| `hck -Ld'   ' -f1,8,19 --no-mmap ./hyper_data_multichar.txt > /dev/null`                                   |  2.289 ± 0.009 |   2.278 |   2.298 |  1.12 ± 0.01 |
| `hck -d'[[:space:]]+' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                     | 10.904 ± 0.100 |  10.805 |  11.033 |  5.36 ± 0.06 |
| `hck -d'[[:space:]]+' --no-mmap -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                           | 11.140 ± 0.162 |  10.871 |  11.314 |  5.48 ± 0.09 |
| `hck -d'\s+' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                              | 11.654 ± 0.018 |  11.625 |  11.670 |  5.73 ± 0.03 |
| `hck -d'\s+' -f1,8,19 --no-mmap ./hyper_data_multichar.txt > /dev/null`                                    | 11.928 ± 0.087 |  11.828 |  12.047 |  5.86 ± 0.05 |
| `choose -f '   ' -i ./hyper_data.txt 0 7 18  > /dev/null`                                                  |  3.255 ± 0.024 |   3.232 |   3.292 |  1.60 ± 0.01 |
| `choose -f '[[:space:]]+' -i ./hyper_data.txt 0 7 18  > /dev/null`                                         | 17.723 ± 0.284 |  17.482 |  18.042 |  8.71 ± 0.15 |
| `choose -f '\s+' -i ./hyper_data.txt 0 7 18  > /dev/null`                                                  | 59.328 ± 0.103 |  59.227 |  59.471 | 29.16 ± 0.17 |
| `awk -F' ' '{print $1, $8 $19}' ./hyper_data_multichar.txt > /dev/null`                                    |  6.795 ± 0.038 |   6.753 |   6.856 |  3.34 ± 0.03 |
| `awk -F'   ' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                                 |  6.017 ± 0.052 |   5.958 |   6.100 |  2.96 ± 0.03 |
| `awk -F'[:space:]+' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                          | 11.099 ± 0.165 |  10.928 |  11.272 |  5.45 ± 0.09 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| cut -d ' ' -f1,8,19 > /dev/null`                                |  7.474 ± 0.019 |   7.455 |   7.500 |  3.67 ± 0.02 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tail -n+2 \| xsv select -d ' ' 1,8,19 --no-headers > /dev/null` |  6.830 ± 0.089 |   6.700 |   6.921 |  3.36 ± 0.05 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| hck -Ld' ' -f1,8,19 > /dev/null`                                |  6.277 ± 0.043 |   6.221 |   6.335 |  3.08 ± 0.03 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tsv-select -d ' ' -f 1,8,19 > /dev/null`                        |  6.313 ± 0.052 |   6.232 |   6.355 |  3.10 ± 0.03 |  |

## Decompression

The following table indicates the file extension / binary pairs that are used to try to decompress a file whent the `-z` option is specified:

| Extension | Binary                   | Type       |
| :-------- | :----------------------- | :--------- |
| `*.gz`    | `gzip -d -c`             | gzip       |
| `*.tgz`   | `gzip -d -c`             | gzip       |
| `*.bz2`   | `bzip2 -d -c`            | bzip2      |
| `*.tbz2`  | `bzip -d -c`             | gzip       |
| `*.xz`    | `xz -d -c`               | xz         |
| `*.txz`   | `xz -d -c`               | xz         |
| `*.lz4`   | `lz4 -d -c`              | lz4        |
| `*.lzma`  | `xz --format=lzma -d -c` | lzma       |
| `*.br`    | `brotli -d -c`           | brotli     |
| `*.zst`   | `zstd -d -c`             | zstd       |
| `*.zstd`  | `zstd -q -d -c`          | zstd       |
| `*.Z`     | `uncompress -c`          | uncompress |

When a file with one of the extensions above is found, `hck` will open a subprocess running the the decompression tool listed above and read from the output of that tool. If the binary can't be found then `hck` will try to read the compressed file as is. See [`grep_cli`](https://github.com/BurntSushi/ripgrep/blob/9eddb71b8e86a04d7048b920b9b50a2e97068d03/crates/cli/src/decompress.rs#L468) for source code. The end goal is to add a similar preprocessor as [ripgrep](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md#preprocessor).

## TODO

- Add complement argument
- Support indexing from the end
- Don't reparse fields / headers for each new file
- figure out how to better reuse / share a vec
- Bake in grep / filtering somehow?
- Move tests from main to core
- Add more tests all around
- Add preprocessor / pigz support
- Add a greedy/non-greedy option that will ignore blank fields `split.filter(|s| !s.is_empty() || config.opt.non_greedy)`
- Implement parallel parser as described [here](https://www.semanticscholar.org/paper/Instant-Loading-for-Main-Memory-Databases-M%C3%BChlbauer-R%C3%B6diger/a1b067fc941d6727169ec18a882080fa1f074595?p2df) This should be very doable given we don't care about escaping quotes and such.

## Questions

I've ripped the code out of the bstr line closure to go faster. The lifetime coercion on the cached vec `shuffler` makes it really hard to break that function because as soon as we start to store things on structs the the compiler realizes what we're doing and throws a fit. Additinally, I haven't found a good way to be generic over an iterator produced by split on regex vs split on bstr. I think the solution might be wrapping in a concrete type but I'm not sure. Overally I'd love for someone who really knows what they are doing to see if they can:

- Fix up the `line_parser.rs` code so that a concrete `LineParser` object can be passed to the `Core` and used to parse lines.
- Work out a better way to reuse the `shuffler` vec, or not use it altogether.

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
- [ripgrep](https://github.com/BurntSushi/ripgrep/tree/master/crates/searcher)
