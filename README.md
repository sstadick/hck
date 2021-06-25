# hck

A close to drop in replacement for cut that uses a regex delmiter instead of a fixed string. This aims to be faster than the equivalent one-liner in awk or perl.

`hck` is a shortening of `hack`, a rougher form of `cut`.

## TODO

- Add complement argument
- Add nice paginated tabular output like `bat` when pipe into terminal

## References

- [rust-coreutils-cut](https://github.com/uutils/coreutils/blob/e48ff9dd9ee0d55da285f99d75f6169a5e4e7acc/src/uu/cut/src/cut.rs)
