//! react-blitz-shot --frames: each frame's timings, then their median, on the CPU renderer.

use std::path::PathBuf;
use std::process::Command;

fn bundle(name: &str) -> PathBuf {
    let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let out = manifest.join("../../target/react-blitz-pages/shot").join(name);
    let status = Command::new("node")
        .arg(manifest.join("../js/scripts/bundle.mjs"))
        .arg(manifest.join("tests/pages").join(name))
        .arg(&out)
        .status()
        .expect("node runs the bundler");
    assert!(status.success(), "bundling {name} failed");
    out
}

/// The numbers before each ` ms` on a line.
fn ms(line: &str) -> Vec<f64> {
    line.split(" ms").filter_map(|part| part.rsplit(' ').next()?.parse().ok()).collect()
}

#[test]
fn three_frames_print_three_timings_and_a_median() {
    let png = std::env::temp_dir().join(format!("rbs-frames-{}.png", std::process::id()));
    let out = Command::new(env!("CARGO_BIN_EXE_react-blitz-shot"))
        .arg(bundle("still"))
        .arg("")
        .arg(&png)
        .args(["--size", "320x200", "--scale", "2", "--timeout", "20"])
        .args(["--frames", "3", "--scroll", "40", "--renderer", "cpu"])
        .output()
        .expect("react-blitz-shot starts");
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));
    let text = String::from_utf8(out.stdout).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 4, "{text}");
    let mut rasters = Vec::new();
    for (i, line) in lines[..3].iter().enumerate() {
        assert!(line.starts_with(&format!("frame {}: scene ", i + 1)), "{line}");
        let [scene, raster] = ms(line)[..] else { panic!("two timings on {line:?}") };
        assert!(scene > 0.0 && raster > 0.0, "{line}");
        rasters.push(raster);
    }
    assert!(lines[3].starts_with("median of 3: scene "), "{text}");
    rasters.sort_by(f64::total_cmp);
    assert_eq!(ms(lines[3])[1], rasters[1], "the median is the middle raster: {text}");
    assert!(std::fs::metadata(&png).unwrap().len() > 1000, "the last frame is written");
}
