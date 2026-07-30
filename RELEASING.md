# Releasing

All crates share one version, set by `workspace.package.version` in the root
`Cargo.toml`.

## GitHub release

1. Bump `workspace.package.version`, run `cargo check --workspace` to update
   `Cargo.lock`, then commit.
2. Tag the commit and publish a GitHub release for the tag.
3. The `Release` workflow runs on `released` and uploads:
   - one archive per platform, with all nine binaries inside, plus a
     `.sha256` file for each
   - one `.deb` per tool, for `amd64` and `arm64`

The workflow builds Linux archives on `ubuntu-22.04` to keep the glibc
requirement low. If that runner image goes away, move to the next oldest one.

## crates.io

Publish in this order, because each step depends on the one before it:

```sh
cargo publish -p ares-device-lib
cargo publish -p ares-connection-lib
for p in ares-package ares-install ares-launch ares-device ares-shell \
         ares-push ares-pull ares-setup-device ares-novacom; do
  cargo publish -p "$p"
done
```

Wait for the index to pick up each library before you publish the crates that
depend on it.
