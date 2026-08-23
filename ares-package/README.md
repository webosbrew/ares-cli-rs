# ares-package

Pack a webOS app directory into an installable `.ipk`.

Part of [ares-cli-rs](https://github.com/webosbrew/ares-cli-rs), a Rust rewrite of
[@webosose/ares-cli](https://github.com/webosose/ares-cli). See the repository
README for install steps.

```text
Pack a webOS app directory into an installable .ipk

Usage: ares-package [OPTIONS] <APP_DIR> [SERVICE_DIR]...

Arguments:
  <APP_DIR>         App directory containing a valid appinfo.json file.
  [SERVICE_DIR]...  Directory containing a valid services.json file

Options:
  -o, --outdir <OUTPUT_DIR>    Use OUTPUT_DIR as the output directory
  -e, --app-exclude <PATTERN>  Exclude files, given as a PATTERN
  -A, --force-arch <ARCH>      Explicitly specify the architecture
  -h, --help                   Print help
```

The architecture is read from the ELF binaries in the app directory. Use
`--force-arch` when there are none, or when the guess is wrong.

## Directory permissions

Every directory in the package is `0777`, which is what `ares-cli` produces: it
forces its umask to `0` and stages the tree with `mkdirp`'s `0777`, so the source
tree's own directory modes never reach the ipk there either.

File modes are taken from the source tree and left alone.

## Examples

```sh
ares-package ./my-app
ares-package ./my-app ./my-service --outdir ./build
ares-package ./my-app --app-exclude '*.map'
```
