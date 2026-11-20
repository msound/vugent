//! Runs the `vugent` binary with VUGENT_* environment variables set.

use std::net::TcpListener;
use std::process::Command;

fn vugent() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_vugent"));
    for (name, _) in std::env::vars_os() {
        if name.to_string_lossy().starts_with("VUGENT_") {
            command.env_remove(name);
        }
    }
    command
}

#[test]
fn help_shows_the_environment_variables() {
    let output = vugent().arg("--help").output().unwrap();
    assert!(output.status.success());
    let help = String::from_utf8(output.stdout).unwrap();
    for name in [
        "VUGENT_HOST",
        "VUGENT_PORT",
        "VUGENT_NO_OPEN",
        "VUGENT_POLL",
    ] {
        assert!(
            help.contains(&format!("[env: {name}=")),
            "{name} missing from:\n{help}"
        );
    }
}

#[test]
fn a_taken_port_from_vugent_port_is_explained() {
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let output = vugent()
        .env("VUGENT_PORT", port.to_string())
        .env("VUGENT_NO_OPEN", "1")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(
        stderr.contains(&format!("port {port} (from VUGENT_PORT) is already in use")),
        "{stderr}"
    );
    assert!(stderr.contains("--port 0"), "{stderr}");
}

#[test]
fn a_flag_wins_over_its_variable() {
    let taken = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    let output = vugent()
        .env("VUGENT_PORT", "not-a-port")
        .args(["--port", &port.to_string(), "--no-open"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .unwrap();
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(stderr.contains("(from --port)"), "{stderr}");
}

/// Whether vugent accepts VUGENT_NO_OPEN=`value`. It runs on a folder that does not exist, so it always fails, but
/// only an unaccepted value fails while parsing the options.
fn accepts_no_open(value: &str) -> bool {
    let output = vugent()
        .env("VUGENT_NO_OPEN", value)
        .arg("/no/such/folder")
        .output()
        .unwrap();
    let stderr = String::from_utf8(output.stderr).unwrap();
    assert!(!output.status.success(), "{stderr}");
    !stderr.contains("invalid value")
}

#[test]
fn on_and_off_variables_take_common_spellings() {
    for value in ["1", "0", "true", "false", "yes", "no", "on", "off"] {
        assert!(accepts_no_open(value), "VUGENT_NO_OPEN={value} was refused");
    }
    assert!(!accepts_no_open("maybe"));
}
