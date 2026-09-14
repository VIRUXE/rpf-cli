use anyhow::{Context, Result};
use std::path::PathBuf;

use rpf_archive::{compose_sheet, encode_image, render_views, DrawableEntry, ImageFormat,
                  LodLevel, RenderOptions, SheetItem, SheetOptions, TextureSet, View};

use crate::resources::{embedded_textures, file_stem, load_drawables, load_texture_dictionary, sanitize};
use crate::rpf::{Archive, GtaKeys};

/// JPEG quality used for the rendered images (PNG/WebP ignore it).
const QUALITY: u8 = 90;

/// Largest grid cell, in pixels, regardless of how big the views are.
const MAX_GRID_CELL: u32 = 512;

#[derive(clap::Args)]
pub struct ScreenshotArgs {
    /// Path to the RPF archive
    pub archive: PathBuf,

    /// Name of a .ydr, .ydd or .yft inside the archive
    pub file: String,

    /// Output directory (default: current directory)
    #[arg(short, long, value_name = "DIR")]
    pub output: Option<PathBuf>,

    /// Texture dictionary to use for textures that are not embedded: a name inside the archive
    /// (".ytd" optional) or a path to a loose .ytd file. Repeatable; earlier ones win.
    #[arg(long, value_name = "NAME|PATH")]
    pub ytd: Vec<String>,

    /// Views to render: front, back, left, right, top, iso (comma separated)
    #[arg(long, value_delimiter = ',', default_value = "iso")]
    pub views: Vec<View>,

    /// Image size as WxH
    #[arg(long, default_value = "1024x1024", value_parser = parse_size)]
    pub size: (u32, u32),

    /// Also combine all views into one labelled grid image
    #[arg(long)]
    pub grid: bool,

    /// LOD to render: high, medium, low, verylow
    #[arg(long, default_value = "high")]
    pub lod: LodLevel,

    /// Image format: png, jpg, webp
    #[arg(short, long, default_value = "png")]
    pub format: ImageFormat,

    /// Background: "grey" (default), "transparent", or #rrggbb
    #[arg(long, default_value = "grey", value_parser = parse_background)]
    pub background: [u8; 4],

    /// Cull back faces (off by default: many GTA surfaces are single-sided planes)
    #[arg(long)]
    pub cull: bool,

    /// Multiply vertex colours into the diffuse
    #[arg(long)]
    pub vertex_colors: bool,

    /// For .ydd/.yft: render only the entry with this name or 0x hash
    #[arg(long)]
    pub entry: Option<String>,

    /// Body colour (#rrggbb) for vehicle paint shaders, which otherwise render white
    #[arg(long, value_name = "#RRGGBB", value_parser = parse_paint)]
    pub paint: Option<[u8; 3]>,
}

/// Parses a `#rrggbb` paint colour.
fn parse_paint(value: &str) -> Result<[u8; 3], String> {
    let hex = value
        .trim()
        .strip_prefix('#')
        .filter(|hex| hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| format!("expected #rrggbb, got '{value}'"))?;

    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0);
    Ok([byte(0), byte(2), byte(4)])
}

/// Parses a `WxH` size such as `"1280x720"`.
fn parse_size(value: &str) -> Result<(u32, u32), String> {
    let (width, height) = value
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("expected WxH (e.g. 1280x720), got '{value}'"))?;

    let parse = |part: &str, what: &str| -> Result<u32, String> {
        let n: u32 = part
            .trim()
            .parse()
            .map_err(|_| format!("invalid {what} '{part}' in '{value}'"))?;
        if n == 0 {
            return Err(format!("{what} must be greater than zero in '{value}'"));
        }
        Ok(n)
    };

    Ok((parse(width, "width")?, parse(height, "height")?))
}

/// Parses a background colour: `grey`/`gray`, `transparent`, or `#rrggbb`.
fn parse_background(value: &str) -> Result<[u8; 4], String> {
    let trimmed = value.trim();

    match trimmed.to_ascii_lowercase().as_str() {
        "grey" | "gray" => return Ok([230, 230, 230, 255]),
        "transparent" => return Ok([0, 0, 0, 0]),
        _ => {}
    }

    let hex = trimmed
        .strip_prefix('#')
        .filter(|hex| hex.len() == 6 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .ok_or_else(|| {
            format!("expected 'grey', 'transparent' or '#rrggbb', got '{value}'")
        })?;

    let byte = |at: usize| u8::from_str_radix(&hex[at..at + 2], 16).unwrap_or(0);
    Ok([byte(0), byte(2), byte(4), 255])
}

/// Strips RAGE's internal `.#dr`/`.#dd`/`.#ft` suffix (marking the resource
/// type of the entry the name was read from) so labels, file names and
/// `--entry` filters see the plain name underneath.
fn strip_rage_suffix(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    for suffix in [".#dr", ".#dd", ".#ft"] {
        if lower.ends_with(suffix) {
            return &name[..name.len() - suffix.len()];
        }
    }
    name
}

/// True when `filter` selects the entry: a case-insensitive name match
/// (ignoring RAGE's internal suffix on either side), or a `0x…` literal equal
/// to the entry's hash.
fn entry_matches(name: &str, hash: u32, filter: &str) -> bool {
    let filter = filter.trim();

    if let Some(hex) = filter.strip_prefix("0x").or_else(|| filter.strip_prefix("0X")) {
        if let Ok(wanted) = u32::from_str_radix(hex, 16) {
            return wanted == hash;
        }
    }

    strip_rage_suffix(name).eq_ignore_ascii_case(strip_rage_suffix(filter))
}

/// Builds an output file name: `<stem>[_<entry>][_<view>].<ext>`. `entry` is
/// given only when several entries are rendered, `view` only when several
/// views are (or for the grid, where it is `"grid"`).
fn image_file_name(stem: &str, entry: Option<&str>, view: Option<&str>, ext: &str) -> String {
    let mut name = stem.to_string();
    if let Some(entry) = entry {
        name.push('_');
        name.push_str(&sanitize(entry));
    }
    if let Some(view) = view {
        name.push('_');
        name.push_str(view);
    }
    format!("{name}.{ext}")
}

/// The label an entry is reported and named by.
fn entry_label(entry: &DrawableEntry) -> String {
    if entry.name.is_empty() {
        format!("0x{:08X}", entry.hash)
    } else {
        strip_rage_suffix(&entry.name).to_string()
    }
}

/// Builds the texture set: embedded textures first, then one layer per `--ytd`.
fn build_texture_set(
    archive: &Archive,
    args: &ScreenshotArgs,
    entries: &[DrawableEntry],
    keys: Option<&GtaKeys>,
) -> TextureSet {
    let mut set = TextureSet::new();

    let embedded: Vec<_> = embedded_textures(entries).into_iter().cloned().collect();
    report_failed(&set.push_layer(&embedded), "embedded");

    if args.ytd.is_empty() {
        let fallback = format!("{}.ytd", file_stem(&args.file));
        match load_texture_dictionary(archive, &fallback, keys) {
            Ok(textures) => {
                println!("Using texture dictionary {} ({} texture(s))", fallback, textures.len());
                report_failed(&set.push_layer(&textures), &fallback);
            }
            Err(err) => println!("No texture dictionary {} found: {}", fallback, err),
        }
        return set;
    }

    for spec in &args.ytd {
        match load_texture_dictionary(archive, spec, keys) {
            Ok(textures) => {
                println!("Using texture dictionary {} ({} texture(s))", spec, textures.len());
                report_failed(&set.push_layer(&textures), spec);
            }
            Err(err) => eprintln!("warning: failed to load texture dictionary '{spec}': {err}"),
        }
    }

    set
}

fn report_failed(failed: &[String], source: &str) {
    if !failed.is_empty() {
        eprintln!("warning: {} texture(s) in {} failed to decode: {}",
                  failed.len(), source, failed.join(", "));
    }
}

pub fn run(args: &ScreenshotArgs, keys: Option<&GtaKeys>) -> Result<()> {
    let archive = Archive::open(&args.archive, keys)?;
    archive.require_keys(keys)?;

    let mut entries = load_drawables(&archive, &args.file, keys)?;

    if let Some(filter) = &args.entry {
        entries.retain(|entry| entry_matches(&entry.name, entry.hash, filter));
        if entries.is_empty() {
            anyhow::bail!("no entry matching '{}' in '{}'", filter, args.file);
        }
    }

    if entries.is_empty() {
        anyhow::bail!("'{}' holds no drawables", args.file);
    }

    let textures = build_texture_set(&archive, args, &entries, keys);

    let stem = file_stem(&args.file);
    let out_dir = args.output.clone().unwrap_or_else(|| PathBuf::from("."));
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("failed to create {}", out_dir.display()))?;

    let views: Vec<View> = if args.views.is_empty() { vec![View::Iso] } else { args.views.clone() };
    let (width, height) = args.size;
    let ext = args.format.extension();
    let many_entries = entries.len() > 1;
    let many_views = views.len() > 1;

    let options = RenderOptions {
        width,
        height,
        view: views[0],
        background: args.background,
        lod: args.lod,
        backface_cull: args.cull,
        vertex_colors: args.vertex_colors,
        paint: args.paint,
        ..Default::default()
    };

    let mut written = 0usize;

    for entry in &entries {
        let label = entry_label(entry);
        let rendered = render_views(&entry.drawable, &textures, &options, &views)
            .with_context(|| format!("failed to render '{}'", label))?;

        let report = rendered
            .first()
            .map(|(_, _, report)| report.clone())
            .unwrap_or_default();

        println!(
            "{}: {} triangles, {} geometries ({} untextured), lod {}, bounds {}",
            label,
            report.triangles,
            report.geometries,
            report.untextured_geometries,
            report.lod.map(|lod| lod.as_str()).unwrap_or("none"),
            if report.bounds_computed { "computed" } else { "from file" },
        );

        if !report.missing_textures.is_empty() {
            println!("Missing textures:");
            for name in &report.missing_textures {
                println!("  {name}");
            }
        }

        let entry_part = many_entries.then(|| label.as_str());

        for (view, image, _) in &rendered {
            let view_part = many_views.then(|| view.label());
            let file_name = image_file_name(&stem, entry_part, view_part, ext);
            let path = out_dir.join(&file_name);
            let encoded = encode_image(image, args.format, QUALITY)?;
            std::fs::write(&path, encoded)
                .with_context(|| format!("failed to write {}", path.display()))?;
            written += 1;
        }

        if args.grid {
            let items: Vec<SheetItem<'_>> = rendered
                .iter()
                .map(|(view, image, _)| SheetItem { label: view.label().to_string(), image })
                .collect();

            // The sheet keeps its own dark background so the view labels stay
            // legible whatever the render background is.
            let sheet_options = SheetOptions {
                cell: width.min(height).min(MAX_GRID_CELL).max(1),
                ..Default::default()
            };

            let sheet = compose_sheet(&items, &sheet_options);
            let encoded = encode_image(&sheet, args.format, QUALITY)?;
            let file_name = image_file_name(&stem, entry_part, Some("grid"), ext);
            let path = out_dir.join(&file_name);
            std::fs::write(&path, encoded)
                .with_context(|| format!("failed to write {}", path.display()))?;
            written += 1;
        }
    }

    println!("Wrote {} image(s) to {}", written, out_dir.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_size_accepts_wxh() {
        assert_eq!(parse_size("1280x720"), Ok((1280, 720)));
        assert_eq!(parse_size("1024X1024"), Ok((1024, 1024)));
        assert_eq!(parse_size("1x1"), Ok((1, 1)));
    }

    #[test]
    fn parse_size_rejects_junk() {
        assert!(parse_size("1280").is_err());
        assert!(parse_size("axb").is_err());
        assert!(parse_size("1280x").is_err());
        assert!(parse_size("0x512").is_err());
        assert!(parse_size("-4x4").is_err());
    }

    #[test]
    fn parse_background_named_colours() {
        assert_eq!(parse_background("grey"), Ok([230, 230, 230, 255]));
        assert_eq!(parse_background("GRAY"), Ok([230, 230, 230, 255]));
        assert_eq!(parse_background("transparent"), Ok([0, 0, 0, 0]));
    }

    #[test]
    fn parse_background_hex() {
        assert_eq!(parse_background("#ff8800"), Ok([255, 136, 0, 255]));
        assert_eq!(parse_background("#FF8800"), Ok([255, 136, 0, 255]));
        assert_eq!(parse_background("#000000"), Ok([0, 0, 0, 255]));
    }

    #[test]
    fn parse_paint_hex_only() {
        assert_eq!(parse_paint("#c81e1e"), Ok([200, 30, 30]));
        assert_eq!(parse_paint("#FFFFFF"), Ok([255, 255, 255]));
        assert!(parse_paint("red").is_err());
        assert!(parse_paint("#fff").is_err());
        assert!(parse_paint("transparent").is_err());
    }

    #[test]
    fn parse_background_rejects_junk() {
        assert!(parse_background("#ff88").is_err());
        assert!(parse_background("ff8800").is_err());
        assert!(parse_background("#gggggg").is_err());
        assert!(parse_background("puce").is_err());
    }

    #[test]
    fn entry_filter_matches_name_case_insensitively() {
        assert!(entry_matches("prop_barrel_01a", 0x1234_5678, "PROP_Barrel_01A"));
        assert!(!entry_matches("prop_barrel_01a", 0x1234_5678, "prop_barrel_01b"));
    }

    #[test]
    fn entry_filter_matches_hash() {
        assert!(entry_matches("prop_barrel_01a", 0x1234_5678, "0x12345678"));
        assert!(entry_matches("prop_barrel_01a", 0x1234_5678, "0X12345678"));
        assert!(!entry_matches("prop_barrel_01a", 0x1234_5678, "0xdeadbeef"));
        // A malformed 0x literal falls back to a name comparison.
        assert!(!entry_matches("prop_barrel_01a", 0x1234_5678, "0xzz"));
    }

    #[test]
    fn strip_rage_suffix_removes_known_suffixes() {
        assert_eq!(strip_rage_suffix("prop_x.#dr"), "prop_x");
        assert_eq!(strip_rage_suffix("prop_x.#DD"), "prop_x");
        assert_eq!(strip_rage_suffix("prop_x.#ft"), "prop_x");
        assert_eq!(strip_rage_suffix("prop_x"), "prop_x");
    }

    #[test]
    fn entry_filter_ignores_rage_suffix_on_either_side() {
        assert!(entry_matches("prop_x.#dr", 0x1234_5678, "prop_x"));
        assert!(entry_matches("prop_x", 0x1234_5678, "prop_x.#dr"));
        assert!(entry_matches("prop_x.#dr", 0x1234_5678, "prop_x.#dr"));
    }

    #[test]
    fn file_names_omit_parts_that_are_not_needed() {
        assert_eq!(image_file_name("prop_x", None, None, "png"), "prop_x.png");
        assert_eq!(image_file_name("prop_x", None, Some("iso"), "png"), "prop_x_iso.png");
        assert_eq!(image_file_name("dict", Some("part_a"), None, "jpg"), "dict_part_a.jpg");
        assert_eq!(
            image_file_name("dict", Some("part_a"), Some("front"), "webp"),
            "dict_part_a_front.webp"
        );
        assert_eq!(image_file_name("prop_x", None, Some("grid"), "png"), "prop_x_grid.png");
    }

    #[test]
    fn file_names_sanitize_entry_names() {
        assert_eq!(image_file_name("d", Some("a b/c.d"), None, "png"), "d_a_b_c_d.png");
        assert_eq!(image_file_name("d", Some(""), None, "png"), "d_entry.png");
    }
}
