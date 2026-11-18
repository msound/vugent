//! The `vugent` command.

use std::net::{Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use tokio::net::TcpListener;

/// See what your agent is doing: review uncommitted changes in your browser.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Folder to review; it must be inside a git repository.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Port to listen on, on 127.0.0.1. 0 picks any free port.
    #[arg(long, default_value_t = 0)]
    port: u16,

    /// Do not open the browser; only print the URL.
    #[arg(long)]
    no_open: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    vugent_git::version().context("vugent needs git; install it and make sure it is on PATH")?;

    let path = args
        .path
        .canonicalize()
        .with_context(|| format!("cannot open {}", args.path.display()))?;
    anyhow::ensure!(path.is_dir(), "{} is not a folder", path.display());
    vugent_git::repo_root(&path)
        .with_context(|| format!("{} is not inside a git repository", path.display()))?;

    let addr = SocketAddr::from((Ipv4Addr::LOCALHOST, args.port));
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("cannot listen on {addr}"))?;
    let url = format!("http://{}/", listener.local_addr()?);

    println!("Reviewing {}", path.display());
    println!("Open {url} (Ctrl-C to stop)");
    if !args.no_open {
        if let Err(err) = webbrowser::open(&url) {
            eprintln!("Could not open a browser ({err}); open the URL yourself.");
        }
    }

    vugent_server::serve(listener, vugent_server::router()).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::CommandFactory;

    #[test]
    fn args_are_valid() {
        Args::command().debug_assert();
    }

    #[test]
    fn path_defaults_to_current_folder() {
        let args = Args::parse_from(["vugent"]);
        assert_eq!(args.path, PathBuf::from("."));
        assert_eq!(args.port, 0);
        assert!(!args.no_open);
    }

    #[test]
    fn flags_parse() {
        let args = Args::parse_from(["vugent", "--port", "7777", "--no-open", "src"]);
        assert_eq!(args.path, PathBuf::from("src"));
        assert_eq!(args.port, 7777);
        assert!(args.no_open);
    }
}
