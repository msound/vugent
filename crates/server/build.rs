//! Checks that the built frontend is present. It is optional for development, but release builds set
//! `VUGENT_REQUIRE_FRONTEND=1` so that a binary is never shipped without the UI.

use std::path::Path;

fn main() {
    println!("cargo::rerun-if-changed=frontend");
    println!("cargo::rerun-if-env-changed=VUGENT_REQUIRE_FRONTEND");

    if Path::new("frontend/index.html").is_file() {
        return;
    }
    let required = std::env::var("VUGENT_REQUIRE_FRONTEND").is_ok_and(|value| value == "1");
    if required {
        panic!(
            "VUGENT_REQUIRE_FRONTEND=1 but the frontend is not built: run `npm ci && npm run build` in web/"
        );
    }
    println!(
        "cargo::warning=the frontend is not built, so the UI will be missing: run `npm install && npm run build` in web/"
    );
}
