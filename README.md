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

## Non-goals

- `hck` does not aim to be a complete CSV / TSV parser a la `xsv` which will respect quoting rules. It acts similar to `cut` in that it will split on the delimiter no matter where in the line it is.
- Delimiters cannot contain newlines... well they can, they will just never be seen. `hck` will always be a line-by-line tool where newlines are the standard `\n` `\r\n`.

## Install

- Homebrew / Linuxbrew

```bash
brew tap sstadick/hck
brew install hck
```

- MacPorts

```bash
sudo port selfupdate
sudo port install hck
```

\* Built with profile guided optimizations

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
rustup component add llvm-tools-preview
git clone https://github.com/sstadick/hck
cd hck
bash pgo_local.sh
cp ./target/release/hck ~/.cargo/bin/hck
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
| `hck -Ld, -f1,8,19 ./hyper_data.txt > /dev/null`             | 1.387 ± 0.019 |   1.369 |   1.407 |        1.00 |
| `hck -Ld, -f1,8,19 --no-mmap ./hyper_data.txt > /dev/null`   | 1.549 ± 0.002 |   1.547 |   1.552 | 1.12 ± 0.02 |
| `hck -d, -f1,8,19  ./hyper_data.txt > /dev/null`             | 1.437 ± 0.001 |   1.436 |   1.438 | 1.04 ± 0.01 |
| `hck -d, -f1,8,19  --no-mmap ./hyper_data.txt > /dev/null`   | 1.706 ± 0.017 |   1.694 |   1.735 | 1.23 ± 0.02 |
| `choose -f , -i ./hyper_data.txt 0 7 18  > /dev/null`        | 4.333 ± 0.063 |   4.254 |   4.384 | 3.12 ± 0.06 |
| `tsv-select -d, -f 1,8,19 ./hyper_data.txt > /dev/null`      | 1.708 ± 0.002 |   1.705 |   1.712 | 1.23 ± 0.02 |
| `xsv select -d, 1,8,19 ./hyper_data.txt > /dev/null`         | 5.600 ± 0.010 |   5.589 |   5.615 | 4.04 ± 0.06 |
| `awk -F, '{print $1, $8, $19}' ./hyper_data.txt > /dev/null` | 4.933 ± 0.059 |   4.901 |   5.038 | 3.56 ± 0.06 |
| `cut -d, -f1,8,19 ./hyper_data.txt > /dev/null`              | 7.421 ± 1.302 |   6.797 |   9.749 | 5.35 ± 0.94 |  |

### Multi-character delimiter benchmark

| Command                                                                                                    |       Mean [s] | Min [s] | Max [s] |     Relative |
| :--------------------------------------------------------------------------------------------------------- | -------------: | ------: | ------: | -----------: |
| `hck -Ld'   ' -f1,8,19 ./hyper_data_multichar.txt > /dev/null` (note, that's three spaces)                 |  1.827 ± 0.009 |   1.818 |   1.842 |         1.00 |
| `hck -Ld'   ' -f1,8,19 --no-mmap ./hyper_data_multichar.txt > /dev/null` (note, that's three spaces)       |  2.123 ± 0.013 |   2.105 |   2.133 |  1.16 ± 0.01 |
| `hck -d'[[:space:]]+' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                     |  9.366 ± 0.202 |   9.009 |   9.506 |  5.13 ± 0.11 |
| `hck -d'[[:space:]]+' --no-mmap -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                           |  9.636 ± 0.030 |   9.588 |   9.666 |  5.27 ± 0.03 |
| `hck -d'\s+' -f1,8,19 ./hyper_data_multichar.txt > /dev/null`                                              | 10.038 ± 0.005 |  10.036 |  10.047 |  5.49 ± 0.03 |
| `hck -d'\s+' -f1,8,19 --no-mmap ./hyper_data_multichar.txt > /dev/null`                                    |  9.913 ± 0.113 |   9.725 |   9.997 |  5.43 ± 0.07 |
| `choose -f '   ' -i ./hyper_data_multichar.txt 0 7 18  > /dev/null`                                        |  6.600 ± 0.071 |   6.555 |   6.723 |  3.61 ± 0.04 |
| `choose -f '[[:space:]]' -i ./hyper_data_multichar.txt 0 7 18  > /dev/null`                                | 10.764 ± 0.041 |  10.703 |  10.809 |  5.89 ± 0.04 |
| `choose -f '\s' -i ./hyper_data_multichar.txt 0 7 18  > /dev/null`                                         | 36.866 ± 0.144 |  36.682 |  37.076 | 20.18 ± 0.13 |
| `awk -F' ' '{print $1, $8 $19}' ./hyper_data_multichar.txt > /dev/null`                                    |  6.602 ± 0.024 |   6.568 |   6.631 |  3.61 ± 0.02 |
| `awk -F'   ' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                                 |  5.894 ± 0.052 |   5.850 |   5.983 |  3.23 ± 0.03 |
| `awk -F'[:space:]+' '{print $1, $8, $19}' ./hyper_data_multichar.txt > /dev/null`                          | 10.962 ± 0.190 |  10.733 |  11.145 |  6.00 ± 0.11 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| cut -d ' ' -f1,8,19 > /dev/null`                                |  7.604 ± 0.096 |   7.521 |   7.730 |  4.16 ± 0.06 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tail -n+2 \| xsv select -d ' ' 1,8,19 --no-headers > /dev/null` |  6.757 ± 0.191 |   6.447 |   6.943 |  3.70 ± 0.11 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| hck -Ld' ' -f1,8,19 > /dev/null`                                |  6.313 ± 0.040 |   6.269 |   6.365 |  3.45 ± 0.03 |
| `< ./hyper_data_multichar.txt tr -s ' ' \| tsv-select -d ' ' -f 1,8,19 > /dev/null`                        |  6.278 ± 0.036 |   6.238 |   6.328 |  3.44 ± 0.03 |

## Decompression

The following table indicates the file extension / binary pairs that are used to try to decompress a file when the `-z` option is specified:

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

## Profile Guided Optimization

See the `pgo*.sh` scripts for how to build this with optimizations. You will need to install the llvm tools via `rustup component add llvm-tools-preview` for this to work. Building with PGO seems to improve performance anywhere from 5-30% depending on the platform and codepath. i.e. on mac os it seems to have a larger effect, and on the regex codepath it also seems to have a greater effect.

## TODO

- Add complement argument
- Don't reparse fields / headers for each new file
- figure out how to better reuse / share a vec
- Support indexing from the end (unlikely though)
- Bake in grep / filtering somehow (this will not be done at the expense of the primary utility of `hck`)
- Move tests from main to core
- Add more tests all around
- Add pigz support
- Add a greedy/non-greedy option that will ignore blank fields `split.filter(|s| !s.is_empty() || config.opt.non_greedy)`
- Experiment with parallel parser as described [here](https://www.semanticscholar.org/paper/Instant-Loading-for-Main-Memory-Databases-M%C3%BChlbauer-R%C3%B6diger/a1b067fc941d6727169ec18a882080fa1f074595?p2df) This should be very doable given we don't care about escaping quotes and such.

## More packages and builds

https://github.com/sharkdp/bat/blob/master/.github/workflows/CICD.yml

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
- [ripgrep](https://github.com/BurntSushi/ripgrep/tree/master/crates/searcher)
