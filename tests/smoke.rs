//! End-to-end smoke test for the image commands against a real GTA V install.
//!
//! Skipped unless `GTAV_PATH` points at the game, since it needs both the
//! retail archives and the keys read out of GTA5.exe.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use rpf_archive::image;

/// Runs the built `rpf` binary and fails the test if it does not exit 0.
fn rpf(args: &[&str]) -> Output {
    let output = Command::new(env!("CARGO_BIN_EXE_rpf"))
        .args(args)
        .output()
        .expect("failed to run the rpf binary");

    assert!(
        output.status.success(),
        "`rpf {}` failed with {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    output
}

fn stdout_of(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

/// Decodes an image the CLI wrote, failing with the path if it is not readable.
fn decode(path: &Path) -> image::RgbaImage {
    let bytes = std::fs::read(path).unwrap_or_else(|err| panic!("{}: {err}", path.display()));
    image::load_from_memory(&bytes)
        .unwrap_or_else(|err| panic!("{} did not decode: {err}", path.display()))
        .to_rgba8()
}

/// Every file in `dir` with the given extension, sorted.
fn files_with_extension(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .unwrap_or_else(|err| panic!("{}: {err}", dir.display()))
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == extension))
        .collect();
    found.sort();
    found
}

/// Depth-first search for a file called `name` somewhere under `dir`.
fn find_file(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in std::fs::read_dir(dir).ok()? {
        let path = entry.ok()?.path();
        if path.is_dir() {
            if let Some(found) = find_file(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|file| file.eq_ignore_ascii_case(name)) {
            return Some(path);
        }
    }
    None
}

#[test]
fn textures_and_screenshot_produce_readable_images() {
    let Ok(gtav) = std::env::var("GTAV_PATH") else {
        println!("GTAV_PATH not set; skipping smoke test");
        return;
    };

    let tmp = tempfile::tempdir().expect("failed to make a temp dir");
    let tex_dir = tmp.path().join("tex");
    let x64a = format!("{gtav}/x64a.rpf");

    // 1. Export a texture dictionary, capped and with a contact sheet.
    rpf(&[
        "textures", &x64a, "binoculars.ytd",
        "-o", tex_dir.to_str().unwrap(), "--sheet", "--max-size", "256",
    ]);

    let sheet = tex_dir.join("binoculars_sheet.png");
    assert!(sheet.is_file(), "the contact sheet is missing");
    decode(&sheet);

    // The cap applies to each exported texture. The contact sheet is a
    // composite of them and is sized by --cell, so it is checked separately.
    let textures: Vec<PathBuf> =
        files_with_extension(&tex_dir, "png").into_iter().filter(|p| *p != sheet).collect();
    assert!(!textures.is_empty(), "no textures written to {}", tex_dir.display());

    for png in &textures {
        let image = decode(png);
        let longest = image.width().max(image.height());
        assert!(longest <= 256, "{} is {longest}px, over the 256px cap", png.display());
    }

    // 2. Render a drawable. Retail archives keep drawables inside nested RPFs,
    //    so one has to come out to disk before it can be opened.
    let nested_dir = tmp.path().join("nested");
    rpf(&[
        "extract", &format!("{gtav}/x64b.rpf"), "*icons.rpf",
        "-o", nested_dir.to_str().unwrap(),
    ]);

    let icons = find_file(&nested_dir, "icons.rpf").expect("icons.rpf was not extracted");
    let icons = icons.to_str().unwrap().to_string();

    let listed = stdout_of(&rpf(&["list", &icons, "*.ydr"]));
    let drawable = listed
        .lines()
        .map(str::trim)
        .find(|line| line.to_ascii_lowercase().ends_with(".ydr"))
        .unwrap_or_else(|| panic!("no .ydr listed in icons.rpf:\n{listed}"));
    let stem = Path::new(drawable).file_stem().unwrap().to_str().unwrap().to_string();

    let shot_dir = tmp.path().join("shot");
    rpf(&[
        "screenshot", &icons, drawable, "-o", shot_dir.to_str().unwrap(),
        "--views", "iso,front", "--grid", "--size", "256x256",
    ]);

    let iso = shot_dir.join(format!("{stem}_iso.png"));
    for name in [format!("{stem}_front.png"), format!("{stem}_grid.png")] {
        decode(&shot_dir.join(name));
    }

    // A render that is nothing but background means nothing was drawn.
    let drawn = decode(&iso).pixels().filter(|pixel| pixel.0 != [230, 230, 230, 255]).count();
    assert!(drawn > 0, "{} is entirely background", iso.display());
}
