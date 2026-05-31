use anyhow::{Context, Result};
use std::{cell::Cell, fs, io::{self, Write}, path::{Path, PathBuf}};
use crate::rpf::{Archive, FileRef, GtaKeys};
use crate::utils::matches_pattern;

pub fn run(archive_path: &Path, output_dir: Option<&Path>, pattern: Option<&str>, recursive: bool, keys: Option<&GtaKeys>) -> Result<()> {
    let archive = Archive::open(archive_path, keys)?;

    let output_path = output_dir.map(Path::to_path_buf).unwrap_or_else(|| {
        PathBuf::from(archive_path.file_stem().and_then(|s| s.to_str()).unwrap_or("extracted"))
    });
    fs::create_dir_all(&output_path)?;

    println!("Archive contains {} entries ({} dirs, {} files)",
        archive.entry_count, archive.dir_count, archive.entry_count - archive.dir_count);

    // Recursive mode: descend into nested RPFs and write every leaf file, preserving the
    // FULL internal directory path (incl. the nested .rpf names as folders) and giving
    // resources a valid RSC7 header. Produces the same loose-file layout as CodeWalker.
    if recursive {
        // Pre-count: walk the whole tree (descending into nested RPFs) up front so we can
        // report the recursive totals before extracting, like CodeWalker does.
        let (total_files, total_resources, nested_rpfs) = count_recursive(&archive, keys, 0);
        println!("Recursive: {} files, {} resources, {} nested rpf(s)",
            total_files, total_resources, nested_rpfs);

        let ok = Cell::new(0usize);
        let fail = Cell::new(0usize);
        extract_recursive(&archive, "", &output_path, pattern, keys, &ok, &fail, 0);
        println!("\n\nExtracted: {} / {}  Failed: {}", ok.get(), total_files, fail.get());
        return Ok(());
    }

    let all_files = archive.list_files();
    let to_extract: Vec<&FileRef> = if let Some(pat) = pattern {
        if !pat.contains('*') && !pat.contains('?') {
            match archive.find_file(pat) {
                Some(f) => vec![f],
                None    => { println!("File not found: {}", pat); return Ok(()); }
            }
        } else {
            all_files.into_iter().filter(|f| matches_pattern(&f.path, pat)).collect()
        }
    } else {
        all_files
    };

    if to_extract.is_empty() {
        println!("No files to extract");
        return Ok(());
    }

    println!("Extracting {} files...", to_extract.len());

    let total = to_extract.len();
    let mut ok = 0usize;
    let mut fail = 0usize;

    for (i, file) in to_extract.iter().enumerate() {
        let dest = output_path.join(&file.path);
        if let Some(parent) = dest.parent() { fs::create_dir_all(parent)?; }

        match archive.extract(file, keys) {
            Ok(data) => {
                fs::write(&dest, &data)
                    .with_context(|| format!("Write failed: {}", dest.display()))?;
                ok += 1;
                print_progress(i + 1, total, &file.name);
            }
            Err(e) => {
                eprintln!("\nFailed to extract {}: {}", file.path, e);
                fail += 1;
            }
        }
    }

    println!("\n\nExtracted: {}  Failed: {}", ok, fail);
    Ok(())
}

/// Recursively count leaf files, resources and nested archives without extracting any
/// file data (it does parse each nested RPF's table of contents). Returns
/// `(leaf_files, resources, nested_rpfs)`. `leaf_files` is the number that will be written.
fn count_recursive(archive: &Archive, keys: Option<&GtaKeys>, depth: usize) -> (usize, usize, usize) {
    const MAX_DEPTH: usize = 16;
    if depth > MAX_DEPTH { return (0, 0, 0); }

    let (mut files, mut resources, mut nested) = (0usize, 0usize, 0usize);
    let refs: Vec<FileRef> = archive.list_files().into_iter().cloned().collect();

    for file in &refs {
        if file.name.to_lowercase().ends_with(".rpf") {
            nested += 1;
            if let Ok(data) = archive.extract(file, keys) {
                if let Ok(child) = Archive::from_bytes(data, &file.name, keys) {
                    let (f, r, n) = count_recursive(&child, keys, depth + 1);
                    files += f;
                    resources += r;
                    nested += n;
                }
            }
        } else {
            files += 1;
            if file.is_resource { resources += 1; }
        }
    }

    (files, resources, nested)
}

/// Recursively extract every file from `archive`, descending into nested .rpf entries.
/// `prefix` is the path of this archive within the parent tree (empty for the root).
#[allow(clippy::too_many_arguments)]
fn extract_recursive(
    archive: &Archive,
    prefix: &str,
    output_path: &Path,
    pattern: Option<&str>,
    keys: Option<&GtaKeys>,
    ok: &Cell<usize>,
    fail: &Cell<usize>,
    depth: usize,
) {
    const MAX_DEPTH: usize = 16;
    if depth > MAX_DEPTH {
        eprintln!("\n[RPF] max nesting depth reached at {}", prefix);
        return;
    }

    // Clone the FileRefs so we don't hold an immutable borrow of `archive` across the
    // nested `Archive::from_bytes` recursion.
    let files: Vec<FileRef> = archive.list_files().into_iter().cloned().collect();

    for file in &files {
        let full = if prefix.is_empty() {
            file.path.clone()
        } else {
            format!("{}/{}", prefix, file.path)
        };

        let data = match archive.extract(file, keys) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("\nFailed to extract {}: {}", full, e);
                fail.set(fail.get() + 1);
                continue;
            }
        };

        if file.name.to_lowercase().ends_with(".rpf") {
            // Nested archive: parse the extracted bytes and recurse under its full path.
            match Archive::from_bytes(data, &file.name, keys) {
                Ok(nested) => extract_recursive(&nested, &full, output_path, pattern, keys, ok, fail, depth + 1),
                Err(e) => {
                    eprintln!("\nFailed to parse nested {}: {}", full, e);
                    fail.set(fail.get() + 1);
                }
            }
            continue;
        }

        if let Some(pat) = pattern {
            if !matches_pattern(&full, pat) { continue; }
        }

        let dest = output_path.join(&full);
        if let Some(parent) = dest.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                eprintln!("\nmkdir failed {}: {}", parent.display(), e);
                fail.set(fail.get() + 1);
                continue;
            }
        }
        match fs::write(&dest, &data) {
            Ok(_) => {
                let n = ok.get() + 1;
                ok.set(n);
                let label = if file.name.len() > 40 { format!("...{}", &file.name[file.name.len() - 37..]) } else { file.name.clone() };
                print!("\r[recursive] extracted {} {:<42}", n, label);
                io::stdout().flush().ok();
            }
            Err(e) => {
                eprintln!("\nWrite failed {}: {}", dest.display(), e);
                fail.set(fail.get() + 1);
            }
        }
    }
}

fn print_progress(n: usize, total: usize, name: &str) {
    let pct = n as f32 / total as f32;
    let filled = (pct * 30.0) as usize;
    let bar = if filled >= 30 {
        "=".repeat(30)
    } else {
        format!("{}>{}",  "=".repeat(filled), " ".repeat(29 - filled))
    };
    let label = if name.len() > 40 { format!("...{}", &name[name.len()-37..]) } else { name.to_string() };
    print!("\r[{}] {}/{} {:<40}", bar, n, total, label);
    io::stdout().flush().ok();
}
