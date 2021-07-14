# Changelog

## v0.5.2-alpha (in progress)

- [PR24](https://github.com/sstadick/hck/pull/24) Removed the now defunct profile guided optimization shell scripts and all references to them in favor of the `justfile` that was added in `v0.5.0`

## v0.5.1

- Fix the version in the binary to match the actual version

## v0.5.0

- Added `--exclude|-e` flag to select a set of fields to exclude. These fields may overlap with the `-f` flag and take precedence over fields selected by `-f`.
- Added `--exclude-header|-E` flag to select a set of headers to exclude. These may mix and match with `-e` `-f` and `F`. The `-r` flag will cause the headers to be treated as a regex.
- As part of the `-e` additional, the default behavior if now headers or fields are specified is to assume `-f1-`, which allow the user to do `hck -e 3,8,290`.
- pigz is now a supported decompression binary, if it's not present `hck` defaults back to `gzip`.
- Decided against adding a greedy heuristic because it actually had worse performance on the most common case of `\s` (but better on `[[:space:]]`, which was odd).
  - The place where this would make sense would be searching a literal space character greedily (like awk), but that kind of goes against the way the delimiters are documented to work
  - It may be worth adding that special case at some point?
- Moves CI to using justfile instead of pgo scripts.
- Fixes several issues in benchmarks
  - `choose` was not using fastest path and had the wrong input file
  - All tools splitting by a space regex were incorrectly parsing the header line in the multichar data, header line is now fixed
- Fixes bug with reordering of only two fields

## v0.4.2

- Fixed deb CI
