//! The `vugent` command.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;
use tokio::net::TcpListener;

/// See what your agent is doing: review its changes and commits in your browser.
#[derive(Debug, Parser)]
#[command(version, about)]
struct Args {
    /// Folder to review; it must be inside a git repository.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Address to listen on, IPv4 or IPv6. Use the server's address, 0.0.0.0 or :: to reach vugent from other
    /// machines; the URL then carries a random access token. SSH port forwarding is safer, since vugent speaks plain
    /// HTTP.
    #[arg(long, value_name = "IP", default_value_t = IpAddr::V4(Ipv4Addr::LOCALHOST))]
    host: IpAddr,

    /// Port to listen on. 0 picks any free port. A fixed port, such as 8000, helps with firewalls that only allow some
    /// ports; ports below 1024 need root.
    #[arg(long, default_value_t = 0)]
    port: u16,

    /// Do not open the browser; only print the URL. The browser is never opened when --host is not a loopback
    /// address.
    #[arg(long)]
    no_open: bool,

    /// Branch that the reviewed branch is compared against. Defaults to the first of origin/HEAD, main and master that
    /// exists.
    #[arg(long, value_name = "REF")]
    base: Option<String>,

    /// Check for changes every few seconds instead of watching files, for file systems that report no changes, such
    /// as network drives or /mnt/c under WSL.
    #[arg(long)]
    poll: bool,
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
    let mut review = vugent_git::Review::new(&path);
    if let Some(base) = &args.base {
        review = review.with_main(base);
        anyhow::ensure!(
            review.main_branch()?.is_some(),
            "cannot find the base branch {base}"
        );
    }

    let addr = SocketAddr::from((args.host, args.port));
    let listener = TcpListener::bind(addr)
        .await
        .with_context(|| format!("cannot listen on {addr}"))?;
    let local_addr = listener.local_addr()?;
    let mut url = url(local_addr, || {
        gethostname::gethostname().to_string_lossy().into_owned()
    });
    // Beyond loopback, anyone who can reach the port could read the code, so a random token is needed (ADR 0009).
    let token = if args.host.is_loopback() {
        None
    } else {
        let token = vugent_server::Token::generate(local_addr.port())
            .context("cannot generate an access token")?;
        url = format!("{url}?token={}", token.as_str());
        Some(token)
    };

    println!("Reviewing {}", path.display());
    println!("Open {url} (Ctrl-C to stop)");
    if token.is_some() {
        eprintln!(
            "Warning: vugent speaks plain HTTP, so the token and your code travel unencrypted on the network. \
             SSH port forwarding is safer: see https://github.com/msound/vugent#running-on-a-remote-server"
        );
    }
    if !args.no_open && args.host.is_loopback() {
        if let Err(err) = webbrowser::open(&url) {
            eprintln!("Could not open a browser ({err}); open the URL yourself.");
        }
    }

    let events = vugent_server::Events::new(review.clone(), args.poll);
    let mut router = vugent_server::router(review, events.clone());
    if let Some(token) = token {
        router = vugent_server::require_token(router, token);
    }
    vugent_server::serve(listener, router, events).await?;
    Ok(())
}

/// The URL to open vugent at, listening on `addr`. An unspecified address (0.0.0.0 or ::) cannot be opened, so the
/// machine's `hostname` is used instead.
fn url(addr: SocketAddr, hostname: impl FnOnce() -> String) -> String {
    if addr.ip().is_unspecified() {
        let hostname = hostname();
        if !hostname.is_empty() {
            return format!("http://{hostname}:{}/", addr.port());
        }
    }
    format!("http://{addr}/")
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
        assert_eq!(args.host, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(args.port, 0);
        assert!(!args.no_open);
        assert_eq!(args.base, None);
        assert!(!args.poll);
    }

    #[test]
    fn flags_parse() {
        let args = Args::parse_from([
            "vugent",
            "--host",
            "::",
            "--port",
            "7777",
            "--no-open",
            "--base",
            "develop",
            "--poll",
            "src",
        ]);
        assert_eq!(args.path, PathBuf::from("src"));
        assert_eq!(args.base.as_deref(), Some("develop"));
        assert_eq!(args.host, IpAddr::V6(std::net::Ipv6Addr::UNSPECIFIED));
        assert_eq!(args.port, 7777);
        assert!(args.no_open);
        assert!(args.poll);
    }

    #[test]
    fn host_takes_ipv4_and_ipv6_but_not_names() {
        let args = Args::parse_from(["vugent", "--host", "0.0.0.0"]);
        assert_eq!(args.host, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        let args = Args::parse_from(["vugent", "--host", "::1"]);
        assert!(args.host.is_loopback());
        assert!(Args::try_parse_from(["vugent", "--host", "localhost"]).is_err());
    }

    fn no_hostname() -> String {
        panic!("the host name is only needed for an unspecified address")
    }

    #[test]
    fn url_uses_the_address() {
        assert_eq!(
            url("127.0.0.1:8000".parse().unwrap(), no_hostname),
            "http://127.0.0.1:8000/"
        );
        assert_eq!(
            url("192.168.1.5:8000".parse().unwrap(), no_hostname),
            "http://192.168.1.5:8000/"
        );
    }

    #[test]
    fn url_puts_ipv6_in_brackets() {
        assert_eq!(
            url("[::1]:8000".parse().unwrap(), no_hostname),
            "http://[::1]:8000/"
        );
    }

    #[test]
    fn url_of_an_unspecified_address_uses_the_host_name() {
        let hostname = || "build-box".to_string();
        assert_eq!(
            url("0.0.0.0:8000".parse().unwrap(), hostname),
            "http://build-box:8000/"
        );
        assert_eq!(
            url("[::]:8000".parse().unwrap(), hostname),
            "http://build-box:8000/"
        );
    }

    #[test]
    fn url_without_a_host_name_falls_back_to_the_address() {
        assert_eq!(
            url("0.0.0.0:8000".parse().unwrap(), String::new),
            "http://0.0.0.0:8000/"
        );
    }
}
