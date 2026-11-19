# FanboxDL Archive

> Check [PostArchiver](https://github.com/xiao-e-yun/PostArchiver) to know more info.

It is importer for fanboxDL to PostArchiver.

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

## Upgrading from v0.1

v0.2 uses post-archiver 0.6. Before running it on an existing archive, upgrade the
archive database with [post-archiver-bridge](https://github.com/xiao-e-yun/PostArchiver) (v0.5 -> v0.6).

Removed options: `--transform` (move mode), `--overwrite` and `--limit`.

## Build

How to build & run code
```sh
cargo run
```
