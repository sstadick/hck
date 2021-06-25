# hck

_`hck` is a shortening of `hack`, a rougher form of `cut`._

A close to drop in replacement for cut that uses a regex delmiter instead of a fixed string. Additionally this tool allows for specification of the order of the output columns using the same column selection syntax as cut (see below for examples).

## Features

- Reordering of output columns! i.e. if you use `-f4,2,8` the output columns will appear in the order `4`, `2`, `8`
- Regex delimiter, i.e. you can split on multiple spaces without and extra pipe to `tr`!
- Selection of columns by header regex with the `-F` option, or by string literal by setting the `-L` flag
- As fast as cut or awk

## TODO

- Add complement argument
- Handle pipe closing and such more gracefully
- Add nice paginated tabular output like `bat` when pipe into terminal

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
