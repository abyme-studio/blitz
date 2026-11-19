//! `react-blitz-shot <dist> <query> <out.png> --size WxH --scale N [--scheme light|dark]
//! [--page index.html] [--timeout SECONDS] [--boxes boxes.json] [--renderer cpu|gpu]
//! [--frames N --scroll PX [--fragments]]`
//!
//! `--renderer` paints with Vello's CPU renderer (the default) or its GPU renderer.
//!
//! With `--frames`: the page scrolled by a wheel of `--scroll` CSS pixels (down when positive)
//! before each of N frames, each painted whole with no window; each frame's scene and raster ms, the
//! scroll containers' fragments it built and kept, the glyph outlines it encoded and text layouts
//! it built, and the medians go to stdout, the last frame to
//! `<out.png>`. `--fragments` keeps each scroll container's content between frames; without it every
//! frame builds its scene whole.

use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

use anyhow::{Context, bail};
use blitz_traits::shell::ColorScheme;
use react_blitz_shot::{Raster, Shot, boxes_json, median, scroll_frames, shoot_on, write_png};

const USAGE: &str = "usage: react-blitz-shot <dist> <query> <out.png> --size WxH --scale N \
[--scheme light|dark] [--page index.html] [--timeout SECONDS] [--boxes boxes.json] \
[--renderer cpu|gpu] [--frames N --scroll PX [--fragments]]";

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
    let mut boxes = None;
    let (mut frames, mut scroll, mut raster) = (None::<usize>, 0.0, Raster::Cpu);
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
            "--boxes" => boxes = Some(PathBuf::from(value("--boxes")?)),
            "--frames" => frames = Some(value("--frames")?.parse().context("--frames is a count")?),
            "--scroll" => scroll = value("--scroll")?.parse().context("--scroll is CSS pixels")?,
            "--renderer" => {
                raster = match value("--renderer")?.as_str() {
                    "cpu" => Raster::Cpu,
                    "gpu" => Raster::Gpu,
                    other => bail!("--renderer is cpu or gpu, not {other}"),
                }
            }
            "--fragments" => blitz_paint::keep_fragments(true),
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
    if let Some(frames) = frames {
        let (times, image) = scroll_frames(&shot, frames, scroll, raster)?;
        for (i, t) in times.iter().enumerate() {
            println!(
                "frame {}: scene {:.2} ms, raster {:.2} ms; fragments built {}, kept {}; glyph outlines {}, text layouts {}",
                i + 1,
                t.scene,
                t.raster,
                t.fragments.built,
                t.fragments.kept,
                t.glyph_outlines,
                t.text_layouts
            );
        }
        println!(
            "median of {}: scene {:.2} ms, raster {:.2} ms",
            times.len(),
            median(times.iter().map(|t| t.scene).collect()),
            median(times.iter().map(|t| t.raster).collect())
        );
        return write_png(&image, out.as_ref());
    }
    let image = shoot_on(&shot, raster)?;
    if let Some(path) = boxes {
        std::fs::write(&path, boxes_json(&image.boxes)).with_context(|| format!("writing {}", path.display()))?;
    }
    write_png(&image, out.as_ref())
}
