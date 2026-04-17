use anyhow::Result;
use std::path::Path;
use crate::rpf::{Archive, DirNode, GtaKeys};

pub fn run(archive_path: &Path, max_depth: Option<usize>, keys: Option<&GtaKeys>) -> Result<()> {
    let archive = Archive::open(archive_path, keys)?;

    println!("{}", archive_path.file_name().unwrap_or_default().to_string_lossy());
    print_tree(&archive.root, "", 0, max_depth);

    let file_count = archive.list_files().len();
    let dir_count  = count_dirs(&archive.root).saturating_sub(1);
    println!("\n{} directories, {} files", dir_count, file_count);

    Ok(())
}

fn print_tree(dir: &DirNode, prefix: &str, depth: usize, max: Option<usize>) {
    if max.map_or(false, |m| depth >= m) { return; }

    let mut subdirs = dir.subdirs.clone();
    subdirs.sort_by(|a, b| a.name.cmp(&b.name));
    let mut files = dir.files.clone();
    files.sort_by(|a, b| a.name.cmp(&b.name));

    let total = subdirs.len() + files.len();
    let mut idx = 0;

    for sub in &subdirs {
        idx += 1;
        let last = idx == total;
        let conn = if last { "└── " } else { "├── " };
        println!("{}{}{}/", prefix, conn, sub.name);
        let child_prefix = format!("{}{}", prefix, if last { "    " } else { "│   " });
        print_tree(sub, &child_prefix, depth + 1, max);
    }

    for f in &files {
        idx += 1;
        let last = idx == total;
        let conn = if last { "└── " } else { "├── " };
        // Resources: show on-disk size; binary files: show uncompressed size
        let display = if f.is_resource { f.size } else { f.mem_size } as u64;
        println!("{}{}{} ({})", prefix, conn, f.name, fmt_size(display));
    }
}

fn count_dirs(dir: &DirNode) -> usize {
    1 + dir.subdirs.iter().map(count_dirs).sum::<usize>()
}

fn fmt_size(n: u64) -> String {
    const U: &[&str] = &["B", "KB", "MB", "GB"];
    let mut v = n as f64;
    let mut i = 0;
    while v >= 1024.0 && i < U.len() - 1 { v /= 1024.0; i += 1; }
    if i == 0 { format!("{} B", n) } else { format!("{:.2} {}", v, U[i]) }
}
