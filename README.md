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
`--no-open` to only print the URL. If the frontend was not built, cargo prints a warning and the binary still runs,
but the UI explains how to build it. Set `VUGENT_REQUIRE_FRONTEND=1` to make the build fail instead.

## Frontend development

The browser UI lives in `web/` (Vite + Svelte 5 + TypeScript). Run the Rust server on a fixed port, then start the
Vite dev server, which proxies `/api` to it:

```sh
cargo run -- --port 7777 --no-open
cd web && npm install && npm run dev
```

Set `VUGENT_SERVER` to proxy to a different address. Before committing, run `npm run check` and `npm run lint`
(`npm run format` fixes formatting).
