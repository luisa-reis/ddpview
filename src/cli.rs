//! The command line. Every flag is optional and overrides, for this run,
//! what the settings file remembers.

use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::{bail, Context, Result};

use crate::forward;
use crate::size::Size;

pub const USAGE: &str = "\
ddpview: a virtual LED panel. Shows the DDP frames sent to it.

Usage: ddpview [--size WxH] [--port N] [--bind ADDRESS] [--forward HOST[:PORT]]
               [--http-port N]

  --size WxH      the panel's size, e.g. 64x64 (otherwise: as remembered,
                  or guessed from the first frame)
  --port N        the UDP port to listen on (otherwise: as remembered, or 4048)
  --bind ADDRESS  the address to listen on. The default, 127.0.0.1, hears
                  only this computer; 0.0.0.0 hears the network
  --forward HOST[:PORT]
                  also send every DDP packet, unchanged, to another DDP
                  receiver (port 4048 unless given). May be given more
                  than once
  --http-port N   the TCP port to answer WLED's API on (otherwise: as
                  remembered, or 80, which WLED's clients expect)
  --help          this text
  --version       the version

--size, --port and --http-port apply to this run; a size or port set in
the window, or http_port in the settings file, is what is remembered. --bind and --forward are never remembered.";

/// Localhost: only programs on this computer can send to it.
pub const DEFAULT_BIND: IpAddr = IpAddr::V4(Ipv4Addr::LOCALHOST);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub size: Option<Size>,
    pub port: Option<u16>,
    pub bind: IpAddr,
    /// Receivers to send every packet on to.
    pub forward: Vec<SocketAddr>,
    pub http_port: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run(Options),
    Help,
    Version,
}

/// Reads the arguments after the program's name.
pub fn parse(args: &[String]) -> Result<Command> {
    let mut options = Options {
        size: None,
        port: None,
        bind: DEFAULT_BIND,
        forward: Vec::new(),
        http_port: None,
    };
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--size" => {
                options.size = Some(
                    it.next()
                        .and_then(|v| Size::parse(v))
                        .context("--size needs WxH, e.g. 128x64")?,
                );
            }
            "--port" => {
                let port = it.next().and_then(|v| v.parse().ok()).filter(|p| *p != 0);
                options.port = Some(port.context("--port needs a number from 1 to 65535")?);
            }
            "--bind" => {
                let bind = it.next().and_then(|v| v.parse().ok());
                options.bind = bind.context("--bind needs an IP address, e.g. 0.0.0.0")?;
            }
            "--forward" => {
                let text = it
                    .next()
                    .context("--forward needs a receiver's address, e.g. 192.168.1.50")?;
                let target = forward::target(text)
                    .with_context(|| format!("--forward cannot send to {text:?}"))?;
                options.forward.push(target);
            }
            "--http-port" => {
                let port = it.next().and_then(|v| v.parse().ok()).filter(|p| *p != 0);
                options.http_port =
                    Some(port.context("--http-port needs a number from 1 to 65535")?);
            }
            "--help" | "-h" => return Ok(Command::Help),
            "--version" | "-V" => return Ok(Command::Version),
            other => bail!("unknown argument {other:?}; --help lists them"),
        }
    }
    Ok(Command::Run(options))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(args: &[&str]) -> Result<Command> {
        parse(&args.iter().map(|a| a.to_string()).collect::<Vec<_>>())
    }

    #[test]
    fn no_flags_is_localhost_and_nothing_overridden() {
        let Command::Run(options) = run(&[]).unwrap() else {
            panic!("runs")
        };
        assert_eq!(
            options,
            Options {
                size: None,
                port: None,
                bind: "127.0.0.1".parse().unwrap(),
                forward: Vec::new(),
                http_port: None,
            }
        );
    }

    #[test]
    fn flags_are_read() {
        let Command::Run(options) = run(&[
            "--size",
            "128x64",
            "--port",
            "4049",
            "--bind",
            "0.0.0.0",
            "--http-port",
            "8080",
        ])
        .unwrap() else {
            panic!("runs")
        };
        assert_eq!(options.http_port, Some(8080));
        assert_eq!(options.size, Some(Size::new(128, 64)));
        assert_eq!(options.port, Some(4049));
        assert!(options.bind.is_unspecified());
        let Command::Run(v6) = run(&["--bind", "::1"]).unwrap() else {
            panic!("runs")
        };
        assert!(v6.bind.is_loopback() && v6.bind.is_ipv6());
        let Command::Run(on) =
            run(&["--forward", "192.168.1.50", "--forward", "127.0.0.1:4049"]).unwrap()
        else {
            panic!("runs")
        };
        assert_eq!(
            on.forward,
            [
                "192.168.1.50:4048".parse().unwrap(),
                "127.0.0.1:4049".parse().unwrap()
            ]
        );
        assert_eq!(run(&["--help"]).unwrap(), Command::Help);
        assert_eq!(
            run(&["--port", "1", "--version"]).unwrap(),
            Command::Version
        );
    }

    #[test]
    fn mistakes_say_what_is_needed() {
        for (args, needs) in [
            (&["--size"][..], "--size needs WxH"),
            (&["--size", "big"], "--size needs WxH"),
            (&["--port", "0"], "--port needs a number"),
            (&["--port", "70000"], "--port needs a number"),
            (&["--http-port", "0"], "--http-port needs a number"),
            (&["--bind", "localhost"], "--bind needs an IP address"),
            (&["--forward"], "--forward needs a receiver's address"),
            (&["--forward", "10.0.0.7:0"], "--forward cannot send to"),
            (&["--fullscreen"], "unknown argument"),
        ] {
            let error = run(args).expect_err("a mistake").to_string();
            assert!(error.contains(needs), "{args:?}: {error}");
        }
    }
}
