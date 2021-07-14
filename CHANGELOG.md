# Changelog

## v0.5.0

- Added `--exclude|-e` flag to select a set of fields to exclude. These fields may overlap with the `-f` flag and take precedence over fields selected by `-f`.
- Added `--exclude-header|-E` flag to select a set of headers to exclude. These may mix and match with `-e` `-f` and `F`. The `-r` flag will cause the headers to be treated as a regex.
- As part of the `-e` additiona, the default behaviour if now headers or fields are specified is to assume `-f1-`, which allow the user to do `hck -e 3,8,290`.
- Added the `--greedy-regex/-g` flag causes delimiter splitting to behave like the awk default, so splitting on `\s` will consume all spaces in a row without specifying `\s+`. This offers a significant performance improvement for greedy regex's.

## v0.4.2

- Fixed deb CI
