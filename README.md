# hck

A close to drop in replacement for cut that uses a regex delmiter instead of a fixed string. This aims to be faster than the equivalent one-liner in awk or perl.

`hck` is a shortening of `hack`, a rougher form of `cut`.


## Features

- As fast or faster than `cut`, lazy splitting
- Regex delimiter, i.e. you can split on multiple spaces without and extra pipe to `tr`!
- Selection of columns by header regex with the `-F` option

## TODO

- Add complement argument
- Allow reordering of outputs
  - Maybe store each column as things are read in order, then reorder and print... but how to not be slow?
- Find a way to treate some headers as literal so they don't have to be wrapped in `^$`
- Add nice paginated tabular output like `bat` when pipe into terminal

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
