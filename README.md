# vugent

## Building

The browser UI is embedded into the `vugent` binary when it is compiled, so build the frontend first. `npm run build`
writes it to `crates/server/frontend/`, where it is also picked up when the crate is published:

```sh
cd web && npm install && npm run build && cd ..
cargo build --release
target/release/vugent .
```

`vugent` listens on a free port on `127.0.0.1` and opens the UI in your browser. Use `--port` to pick the port and
`--no-open` to only print the URL. The branch is compared against the first of `origin/HEAD`, `main` and `master`
that exists; use `--base <ref>` to compare against another branch. The UI follows changes as they happen by
watching the folder; use `--poll` to check every few seconds instead, on file systems that report no changes, such as
network drives or `/mnt/c` under WSL. If the frontend was not built, cargo prints a
warning and the binary still runs, but the UI explains how to build it. Set `VUGENT_REQUIRE_FRONTEND=1` to make the
build fail instead.

## Frontend development

The browser UI lives in `web/` (Vite + Svelte 5 + TypeScript). Run the Rust server on a fixed port, then start the
Vite dev server, which proxies `/api` to it:

```sh
cargo run -- --port 7777 --no-open
cd web && npm install && npm run dev
```

Set `VUGENT_SERVER` to proxy to a different address. Before committing, run `npm run check` and `npm run lint`
(`npm run format` fixes formatting).

## Releasing

Releases are made with [cocogitto](https://docs.cocogitto.io/) from an up-to-date `main` (it also needs
[cargo-edit](https://github.com/killercup/cargo-edit) for `cargo set-version`):

```sh
cog bump --auto
```

This picks the next version from the conventional commits since the last tag, sets it in `Cargo.toml`, updates
`CHANGELOG.md`, makes a `chore(version): vX.Y.Z` commit, tags it, and pushes both the commit and the tag. Its
configuration is in `cog.toml`.

The pushed tag runs the cargo-dist release workflow (`.github/workflows/release.yml`), which builds the frontend, then
the binaries and shell/PowerShell installers, and publishes them to a GitHub Release. The cargo-dist configuration is
in `dist-workspace.toml` and `.github/build-setup.yml`; run `dist generate` after changing them.
