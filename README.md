# FanboxDL Archive

> Check [PostArchiver](https://github.com/xiao-e-yun/PostArchiver) to know more info.

It is importer for fanboxDL to PostArchiver.

```sh
Usage: fanbox-dl-archive [OPTIONS] <INPUT> [OUTPUT]

Arguments:
  <INPUT>   Your fanbox dl archive path [env: INPUT=]
  [OUTPUT]  Which you path want to save [env: OUTPUT=] [default: ./archive]

Options:
  -o, --overwrite                   Overwrite existing files
  -t, --transform <TRANSFORM>       Transform method [default: copy] [possible values: copy, move, hardlink]
  -w, --whitelist [<WHITELIST>...]  Whitelist of creator IDs
  -b, --blacklist [<BLACKLIST>...]  Blacklist of creator IDs
  -l, --limit <LIMIT>               Limit the number of concurrent copys [default: 5]
  -v, --verbose...                  Increase logging verbosity
  -q, --quiet...                    Decrease logging verbosity
  -h, --help                        Print help
```

## Build

How to build & run code
```sh
cargo run
```
