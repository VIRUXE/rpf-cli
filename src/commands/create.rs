use anyhow::{bail, Result};
use rage_rpf::{RpfBuilder, RpfEncryption, RpfVersion};
use std::fs;
use std::path::Path;

use rage_rpf::GtaKeys;

pub fn run(
    input_dir: &Path,
    output: &Path,
    version: u8,
    encryption: &str,
    keys: Option<&GtaKeys>,
) -> Result<()> {
    if !input_dir.is_dir() {
        bail!("{} is not a directory", input_dir.display());
    }

    let rpf_version = match version {
        0 => RpfVersion::V0,
        2 => RpfVersion::V2,
        3 => RpfVersion::V3,
        4 => RpfVersion::V4,
        6 => RpfVersion::V6,
        7 => RpfVersion::V7,
        _ => bail!("unsupported version {}; valid: 0 2 3 4 6 7", version),
    };

    let rpf_encryption = match encryption {
        "none" => RpfEncryption::None,
        "open" => RpfEncryption::Open,
        "ng"   => RpfEncryption::Ng,
        other  => bail!("unknown encryption '{}'; valid: none open ng", other),
    };

    let mut builder = RpfBuilder::for_version(rpf_version, rpf_encryption);

    add_dir(&mut builder, input_dir, input_dir)?;

    let data = builder.build(keys)?;
    fs::write(output, &data)?;

    println!("Created {} ({} bytes)", output.display(), data.len());
    Ok(())
}

fn add_dir(builder: &mut RpfBuilder, base: &Path, dir: &Path) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            add_dir(builder, base, &path)?;
        } else {
            let rel = path.strip_prefix(base)?;
            let archive_path = rel.to_string_lossy().replace('\\', "/");
            let data = fs::read(&path)?;
            builder.add_file(&archive_path, data);
        }
    }
    Ok(())
}
