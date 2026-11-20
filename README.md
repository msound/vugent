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
`--no-open` to only print the URL. Use `--host` to listen on another address, such as `0.0.0.0` (see
[Running on a remote server](#running-on-a-remote-server)). The branch is compared against the first of `origin/HEAD`, `main` and `master`
that exists; use `--base <ref>` to compare against another branch. The UI follows changes as they happen by
watching the folder; use `--poll` to check every few seconds instead, on file systems that report no changes, such as
network drives or `/mnt/c` under WSL. If the frontend was not built, cargo prints a
warning and the binary still runs, but the UI explains how to build it. Set `VUGENT_REQUIRE_FRONTEND=1` to make the
build fail instead.

The UI follows your system's light or dark setting. The sun and moon buttons next to the vugent title in the
bottom-left corner force the light or dark theme, and the button between them follows the system again.

## Running on a remote server

When vugent runs on a server you reach over SSH, the safest way to open it is to forward its port, so that vugent
stays on `127.0.0.1` and the connection is encrypted by SSH:

```sh
ssh -L 8000:127.0.0.1:8000 server
vugent --port 8000 --no-open
```

Then open `http://localhost:8000/` on your laptop. VS Code Remote-SSH forwards ports for you automatically.

Otherwise, `vugent --host 0.0.0.0 --port 8000` listens on every address of the server and prints a URL with its host
name and a random access token, such as `http://server:8000/?token=…`. Open that URL; the browser then keeps the token
in a cookie, and every request without it is refused. If the host name cannot be reached from your machine, pass the
server's IP address instead, such as `--host 192.168.1.5`, and the URL uses that address. A new token is made every
time vugent starts, so open the new URL after a restart. Pick a fixed port that your firewall allows; ports below 1024
need root. The browser is not opened, since it is on another machine. Note that vugent speaks plain HTTP, so the token
and the code travel unencrypted on the network.

To avoid typing the same options on every run, set them once in `.bashrc` or `.zshrc` with `VUGENT_HOST`,
`VUGENT_PORT`, `VUGENT_NO_OPEN` and `VUGENT_POLL`; an option on the command line still wins. For example, to match the
SSH port forwarding above:

```sh
export VUGENT_PORT=8000 VUGENT_NO_OPEN=1
```

A fixed port also keeps the browser's origin the same from run to run, so it remembers your preferences, such as the
diff layout and the theme. Only one vugent can listen on a port, so pass `--port 0` to a second one to pick any free port. There is
no variable for the access token, which is always random.

## Frontend development

The browser UI lives in `web/` (Vite + Svelte 5 + TypeScript). Run the Rust server on a fixed port, then start the
Vite dev server, which proxies `/api` to it:

```sh
cargo run -- --port 7777 --no-open
cd web && npm install && npm run dev
```

Set `VUGENT_SERVER` to proxy to a different address. Before committing, run `npm run check`, `npm run lint` and
`npm test` (`npm run format` fixes formatting).

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
