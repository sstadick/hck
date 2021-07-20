# Changelog

## v0.5.5 - IN PROGRESS

- Speed up edge case where columns may have already been consumed i.e. `-f1-3,5,2-`

## v0.5.4

- [Bugfix](https://github.com/sstadick/hck/issues/30) Better handling of duplicate selected fields, fixed output ordering when duplicate fields were selected. Added clarification to README regarding mixing by-index and by-header field selction / reordering.

## v0.5.3

- Bugfix, allow headers specified to be excluded to not be found

## v0.5.2

- [PR24](https://github.com/sstadick/hck/pull/24) Removed the now defunct profile guided optimization shell scripts and all references to them in favor of the `justfile` that was added in `v0.5.0`
- [Bugfix](https://github.com/sstadick/hck/issues/26) fixes incorrect handling of header line for non-stdin inputs, fixes incorrect parsing of last header fields (now strips newline before matching), fixes option parsing so that the `-F` and `-E` options wont' try to consume the positional input arguments. Huge thanks to @learnbyexample for their detailed bug report.
- Change: An error will now be raised when a specified header is not found. This differs from the convention used by the selecion-by-index, which tries to match `cut`. The reasoning is that it is generally harder to type out each header field and if a header is not found you want to know about it.

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
