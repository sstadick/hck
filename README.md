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
- Delimiter treated as a regex, i.e. you can split on multiple spaces without and extra pipe to `tr`!
- Specification of output delimiter
- Selection of columns by header string literal with the `-F` option, or by regex by setting the `-r` flag
- Input files will be automatically decompressed if their file extension is recognizable and a local binary exists to perform the decompression (similar to ripgrep). See [Decompression](#decompression).
- Exclude fields by index or by header.
- Speed

## Non-goals

- `hck` does not aim to be a complete CSV / TSV parser a la `xsv` which will respect quoting rules. It acts similar to `cut` in that it will split on the delimiter no matter where in the line it is.
- Delimiters cannot contain newlines... well they can, they will just never be seen. `hck` will always be a line-by-line tool where newlines are the standard `\n` `\r\n`.

## Install

- Homebrew / Linuxbrew

```bash
brew tap sstadick/hck
brew install hck
```

\* Built with profile guided optimizations

- MacPorts

```bash
# Note, version may lag behind latest
sudo port selfupdate
sudo port install hck
```

- Debian (Ubuntu)

```bash
curl -LO https://github.com/sstadick/hck/releases/download/<latest>/hck-linux-amd64.deb
sudo dpkg -i hck-linux-amd64.deb
```

\* Built with profile guided optimizations

- With the Rust toolchain:

```bash
export RUSTFLAGS='-C target-cpu=native'
cargo install hck
```

- From the [releases page](https://github.com/sstadick/hck/releases) (the binaries have been built with profile guided optimizations)

- Or, if you want the absolute fastest possible build that makes use of profile guided optimizations AND native cpu features:

```bash
# Assumes you are on stable rust
# NOTE: this won't work on windows, see CI for linked issue
cargo install just
git clone https://github.com/sstadick/hck
cd hck
just install-native
```

- PRs are both welcome and encouraged for adding more packaging options and build types! I'd especially welcome PRs for the windows family of package managers / general making sure things are windows friendly.

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

### Excluding output columns

```bash
❯ ps aux | hck -e3,5 | head -n4
USER    PID     %MEM    RSS     TTY     STAT    START   TIME    COMMAND
root    1       0.0     14408   ?       Ss      Jun21   0:27    /sbin/init      splash
root    2       0.0     0       ?       S       Jun21   0:01    [kthreadd]
root    3       0.0     0       ?       I<      Jun21   0:00    [rcu_gp]
```

### Excluding output columns by header regex

```bash
❯  ps aux | hck -r -E "CPU" -E "^ST.*" | head -n4
USER    PID     %MEM    VSZ     RSS     TTY     TIME    COMMAND
root    1       0.0     170224  14408   ?       0:27    /sbin/init      splash
root    2       0.0     0       0       ?       0:01    [kthreadd]
root    3       0.0     0       0       ?       0:00    [rcu_gp]
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
  - v1.3.2 (compiled locally with optimizations)

### Single character delimiter benchmark

| Command                                                      |      Mean [s] | Min [s] | Max [s] |    Relative |
| :----------------------------------------------------------- | ------------: | ------: | ------: | ----------: |
| `hck -Ld, -f1,8,19 ./hyper_data.txt > /dev/null`             | 1.464 ± 0.002 |   1.462 |   1.466 |        1.00 |
| `hck -Ld, -f1,8,19 --no-mmap ./hyper_data.txt > /dev/null`   | 1.642 ± 0.003 |   1.640 |   1.648 | 1.12 ± 0.00 |
| `hck -d, -f1,8,19  ./hyper_data.txt > /dev/null`             | 1.527 ± 0.002 |   1.525 |   1.529 | 1.04 ± 0.00 |
| `hck -d, -f1,8,19  --no-mmap ./hyper_data.txt > /dev/null`   | 1.887 ± 0.004 |   1.883 |   1.893 | 1.29 ± 0.00 |
| `choose -f , -i ./hyper_data.txt 0 7 18  > /dev/null`        | 4.323 ± 0.034 |   4.293 |   4.370 | 2.95 ± 0.02 |
| `tsv-select -d, -f 1,8,19 ./hyper_data.txt > /dev/null`      | 1.723 ± 0.003 |   1.719 |   1.728 | 1.18 ± 0.00 |
| `xsv select -d, 1,8,19 ./hyper_data.txt > /dev/null`         | 5.569 ± 0.004 |   5.563 |   5.573 | 3.80 ± 0.01 |
| `awk -F, '{print $1, $8, $19}' ./hyper_data.txt > /dev/null` | 4.916 ± 0.014 |   4.897 |   4.933 | 3.36 ± 0.01 |
| `cut -d, -f1,8,19 ./hyper_data.txt > /dev/null`              | 6.792 ± 0.017 |   6.769 |   6.813 | 4.64 ± 0.01 |

### Multi-character delimiter benchmark

| Command                                                                                       |       Mean [s] | Min [s] | Max [s] |     Relative |
| :-------------------------------------------------------------------------------------------- | -------------: | ------: | ------: | -----------: |
| `hck -Ld'   ' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                |  1.718 ± 0.003 |   1.715 |   1.722 |         1.00 |
| `hck -Ld'   ' -f1,8,19 --no-mmap ./hyper_data_multichar.txt > /dev/null`                      |  2.191 ± 0.072 |   2.135 |   2.291 |  1.28 ± 0.04 |
| `hck -d'   ' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                 |  2.180 ± 0.029 |   2.135 |   2.208 |  1.27 ± 0.02 |
| `hck -d'   ' --no-mmap -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                       |  2.542 ± 0.014 |   2.529 |   2.565 |  1.48 ± 0.01 |
| `hck -d'[[:space:]]+' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                        |  8.597 ± 0.023 |   8.575 |   8.631 |  5.00 ± 0.02 |
| `hck -d'[[:space:]]+' --no-mmap -f1,8,19 ./hyper_data_multichar.txt > /dev/null`              |  8.890 ± 0.013 |   8.871 |   8.903 |  5.17 ± 0.01 |
| `hck -d'\s+' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                 | 10.014 ± 0.247 |   9.844 |  10.449 |  5.83 ± 0.14 |
| `hck -d'\s+' -f1,8,19 --no-mmap ./hyper_data_multichar.txt > /dev/null`                       | 10.173 ± 0.035 |  10.111 |  10.193 |  5.92 ± 0.02 |
| `choose -f '   ' -i ./hyper_data_multichar.txt 0 7 18  > /dev/null`                           |  6.537 ± 0.148 |   6.452 |   6.799 |  3.80 ± 0.09 |
| `choose -f '[[:space:]]' -i ./hyper_data_multichar.txt 0 7 18  > /dev/null`                   | 10.656 ± 0.219 |  10.484 |  10.920 |  6.20 ± 0.13 |
| `choose -f '\s' -i ./hyper_data_multichar.txt 0 7 18  > /dev/null`                            | 37.238 ± 0.153 |  37.007 |  37.383 | 21.67 ± 0.10 |
| `awk -F' ' '{print $1, $8 $19}' ./hyper_data_multichar.txt > /dev/null`                       |  6.673 ± 0.064 |   6.595 |   6.734 |  3.88 ± 0.04 |
| `awk -F'   ' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                    |  5.947 ± 0.098 |   5.896 |   6.121 |  3.46 ± 0.06 |
| `awk -F'[:space:]+' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`             | 11.080 ± 0.215 |  10.881 |  11.376 |  6.45 ± 0.13 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| cut -d ' ' -f1,8,19 > /dev/null`                   |  7.471 ± 0.066 |   7.397 |   7.561 |  4.35 ± 0.04 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| xsv select -d ' ' 1,8,19 --no-headers > /dev/null` |  6.172 ± 0.068 |   6.071 |   6.235 |  3.59 ± 0.04 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| hck -Ld' ' -f1,8,19 > /dev/null`                   |  6.171 ± 0.112 |   5.975 |   6.243 |  3.59 ± 0.07 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tsv-select -d ' ' -f 1,8,19 > /dev/null`           |  6.202 ± 0.130 |   5.984 |   6.290 |  3.61 ± 0.08 |

## Decompression

The following table indicates the file extension / binary pairs that are used to try to decompress a file when the `-z` option is specified:

| Extension | Binary                   | Type       |
| :-------- | :----------------------- | :--------- |
| `*.gz`    | `pigz -d -c`             | gzip       |
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

When a file with one of the extensions above is found, `hck` will open a subprocess running the the decompression tool listed above and read from the output of that tool. If the binary can't be found then `hck` will try to read the compressed file as is. See [`grep_cli`](https://github.com/BurntSushi/ripgrep/blob/9eddb71b8e86a04d7048b920b9b50a2e97068d03/crates/cli/src/decompress.rs#L468) for source code. The end goal is to add a similar preprocessor as [ripgrep](https://github.com/BurntSushi/ripgrep/blob/master/GUIDE.md#preprocessor). Where there are multiple binaries for a given type, they are tried in the order listed above.

## Profile Guided Optimization

See the `pgo*.sh` scripts for how to build this with optimizations. You will need to install the llvm tools via `rustup component add llvm-tools-preview` for this to work. Building with PGO seems to improve performance anywhere from 5-30% depending on the platform and codepath. i.e. on mac os it seems to have a larger effect, and on the regex codepath it also seems to have a greater effect.

## TODO

- Add output compression detection when writing to a file
- Don't reparse fields / headers for each new file
- Figure out how to better reuse / share a vec
- Support indexing from the end (unlikely though)
- Bake in grep / filtering somehow (this will not be done at the expense of the primary utility of `hck`)
- Move tests from main to core
- Add more tests all around
- Experiment with parallel parser as described [here](https://www.semanticscholar.org/paper/Instant-Loading-for-Main-Memory-Databases-M%C3%BChlbauer-R%C3%B6diger/a1b067fc941d6727169ec18a882080fa1f074595?p2df) This should be very doable given we don't care about escaping quotes and such.

## More packages and builds

https://github.com/sharkdp/bat/blob/master/.github/workflows/CICD.yml

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
- [ripgrep](https://github.com/BurntSushi/ripgrep/tree/master/crates/searcher)
