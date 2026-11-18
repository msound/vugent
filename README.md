# vugent
## Frontend development

The browser UI lives in `web/` (Vite + Svelte 5 + TypeScript). Run the Rust server on a fixed port, then start the
Vite dev server, which proxies `/api` to it:

```sh
cargo run -- --port 7777
cd web && npm install && npm run dev
```

Set `VUGENT_SERVER` to proxy to a different address. Before committing, run `npm run check` and `npm run lint`
(`npm run format` fixes formatting).
