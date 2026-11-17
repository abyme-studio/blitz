//! `react-blitz-shot <dist> <query> <out.png> --size WxH --scale N [--scheme light|dark]
//! [--page index.html] [--timeout SECONDS]`

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, bail};
use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Shot, shoot, write_png};

const USAGE: &str = "usage: react-blitz-shot <dist> <query> <out.png> --size WxH --scale N \
[--scheme light|dark] [--page index.html] [--timeout SECONDS]";

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("react-blitz-shot: {e:#}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let mut plain = Vec::new();
    let (mut width, mut height, mut scale) = (1280, 800, 1.0);
    let mut scheme = ColorScheme::Light;
    let mut page = "index.html".to_string();
    let mut timeout = 30.0;
    while let Some(arg) = args.next() {
        let mut value = |flag: &str| args.next().with_context(|| format!("{flag} needs a value\n{USAGE}"));
        match arg.as_str() {
            "--size" => {
                let v = value("--size")?;
                let (w, h) = v.split_once('x').with_context(|| format!("--size is WxH, not {v}"))?;
                width = w.parse().with_context(|| format!("--size width {w}"))?;
                height = h.parse().with_context(|| format!("--size height {h}"))?;
            }
            "--scale" => scale = value("--scale")?.parse().context("--scale is a number")?,
            "--scheme" => {
                scheme = match value("--scheme")?.as_str() {
                    "light" => ColorScheme::Light,
                    "dark" => ColorScheme::Dark,
                    other => bail!("--scheme is light or dark, not {other}"),
                }
            }
            "--page" => page = value("--page")?,
            "--timeout" => timeout = value("--timeout")?.parse().context("--timeout is seconds")?,
            flag if flag.starts_with("--") => bail!("no flag {flag}\n{USAGE}"),
            _ => plain.push(arg),
        }
    }
    let [dist, query, out] = <[String; 3]>::try_from(plain).map_err(|_| anyhow::anyhow!(USAGE))?;
    let shot = Shot {
        dist: PathBuf::from(dist),
        page,
        query: query.trim_start_matches('?').to_string(),
        width,
        height,
        scale,
        scheme,
        timeout: Duration::from_secs_f64(timeout),
    };
    let image = shoot(&shot)?;
    write_png(&image, out.as_ref())
}
