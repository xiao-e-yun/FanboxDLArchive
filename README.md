# FanboxDL Archive

> Check [PostArchiver](https://github.com/xiao-e-yun/PostArchiver) to know more info.

It imports folders downloaded by [fanbox-dl](https://github.com/hareku/fanbox-dl) into PostArchiver.

```sh
Usage: fanbox-dl-archive [OPTIONS] <INPUT> [OUTPUT]

Arguments:
  <INPUT>   Your fanbox dl archive path [env: INPUT=]
  [OUTPUT]  Which you path want to save [env: OUTPUT=] [default: ./archive]

Options:
      --strategy <STRATEGY>         Archiving strategy (increment: skip posts whose files are all imported) [default: increment]
  -w, --whitelist [<WHITELIST>...]  Whitelist of creator IDs
  -b, --blacklist [<BLACKLIST>...]  Blacklist of creator IDs
  -v, --verbose...                  Increase logging verbosity
  -q, --quiet...                    Decrease logging verbosity
  -h, --help                        Print help
```

Files are always copied into the archive; the input folder is left untouched.

```sh
fanbox-dl-archive ./images ./archive -w creator1 creator2
```

Put `<INPUT> [OUTPUT]` before `-w`/`-b`, since those options take every value after them.

## Supported layouts

Every fanbox-dl layout is split into one post per fanbox post:

| fanbox-dl flags | Layout |
|---|---|
| (default) | `creator/2006-01-02-<title>-<order>-<id>.<ext>` |
| `--dir-by-post` | `creator/2006-01-02-<title>/<order>-<id>.<ext>` |
| `--dir-by-plan` | `creator/<fee>yen/...` (either layout above inside), the fee becomes a tag |

Files that don't match (e.g. names truncated with `...`) go into a single "Fanbox archive" post per creator.
Posts with the same date and title are merged, as fanbox-dl paths carry no post id.

## Upgrading from v0.1

v0.2 uses post-archiver 0.6. Before running it on an existing archive, upgrade the
archive database with [post-archiver-bridge](https://github.com/xiao-e-yun/PostArchiver) (v0.5 -> v0.6).

Removed options: `--transform` (move mode), `--overwrite` and `--limit`.

v0.1 put all files of a creator (or plan) into one post. v0.2 splits them into separate posts
but does not delete the old one, so the files would show up twice. Importing into a new
output folder is recommended.

## Build

Requires Rust 1.91+.

```sh
cargo run -- <INPUT> [OUTPUT]
```
