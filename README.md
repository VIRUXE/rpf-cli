# RPF-CLI

A fast, safe, and cross-platform command-line tool for working with RAGE Package Files (RPF), written in Rust.

Currently, this tool is a work in progress and is not yet ready for production use.

It also only supports RPF7 archives (GTA V)

![image](https://github.com/user-attachments/assets/304c25c9-b338-46d2-b495-42fa73722a61)
![image](https://github.com/user-attachments/assets/ad968510-9413-45ba-9687-3c636b24a299)

Drop a star if you've found this tool useful.

## Usage

```
info          Display information about an archive
list          List files, optionally filtered by pattern
search        Find files by name, contents or hash inside archives (and nested archives) without extracting
extract       Extract files, with --recursive to descend into nested archives
verify        Verify archive integrity
tree          Display contents as a tree
textures      Export textures from a .ytd/.ydr/.ydd/.yft as PNG/JPG/WebP (alias: ytd; --dds for raw DDS)
screenshot    Render a .ydr/.ydd/.yft to an image (auto-framed, multi-view, external --ytd)
create        Create an archive from a directory
extract-keys  Write the keys out to disk for reuse with --keys
```

Reading retail game archives needs keys — see below. Archives that are not
encrypted, such as FiveM resource packs, need no setup at all:

```sh
rpf tree resource.rpf
rpf extract resource.rpf -o ./out
```

## Searching without extracting

`list` only sees one archive's top level; `search` descends into nested `.rpf`
entries in memory, and takes a directory to cover every archive under it:

```sh
rpf search "<GTA V>/x64b.rpf" prop_mk_arrow_3d.ydr          # which nested rpf holds it?
rpf search "<GTA V>" "*.ymt" --json                          # every .ymt in the install
rpf search "<GTA V>/x64a.rpf" --content binoculars -i        # bytes inside files (resources are inflated)
rpf search "<GTA V>/x64b.rpf" --hex "52 53 43 37" --limit 5  # raw byte pattern
rpf search "<GTA V>/x64a.rpf" --hash 0x6D8A1F3C              # JOAAT of a name or stem
```

Hits print as `<archive>:<path/inside/nested.rpf/file>`, one per line; `-d` adds
size, type and hash, and `--json` emits an array with the same fields plus the
match offset. A pattern without wildcards is a substring match on the full path,
so `inner.rpf` returns the archive and everything under it. Filters combine with
AND, and the summary goes to stderr so stdout stays clean for piping.

## Images for humans and LLMs

Both image commands write ordinary picture files, so you can look at a model or
a texture without a modelling tool, and a vision model can read the result.

```sh
rpf textures "<GTA V>/x64a.rpf" binoculars.ytd                      # PNGs into ./binoculars
rpf textures "<GTA V>/x64a.rpf" binoculars.ytd --format webp --max-size 512 --sheet
rpf extract "<GTA V>/x64b.rpf" -o ./nested "*icons.rpf"
rpf screenshot ./nested/levels/gta5/generic/icons.rpf prop_mk_arrow_3d.ydr --views front,iso --grid
rpf textures  ./nested/levels/gta5/generic/icons.rpf prop_mk_arrow_3d.ydr
```

Retail `x64*.rpf` archives keep drawables inside nested RPFs, so a drawable has
to be extracted to disk before either command can open it — `rpf search` tells
you which nested archive holds it, and the extracted path mirrors that
archive's own folder layout (using `GTAV_PATH="C:/Program Files (x86)/Steam/steamapps/common/Grand Theft Auto V"`
above).

Line by line: every texture in a dictionary lands in a folder named after it;
`--sheet` adds one labelled contact sheet of the lot, here as WebP capped at
512 px; a drawable works the same way, exporting the textures baked into it;
`screenshot` renders the model itself from as many angles as you name and
`--grid` collects them into a single labelled image; `--ytd` supplies the
textures a drawable references but does not carry; and `--size` sets the
resolution of each view. The `--grid` sheet always uses its own dark
background, regardless of what `--background` is set to.

PNG is lossless and the best default for vision models. WebP output is
lossless-only, JPEG drops the alpha channel, and `--max-size` keeps files
small without costing you anything — models downscale past roughly 1500 px
anyway. Textures a model asks for but no dictionary supplies are drawn flat
grey and listed by name, so the output itself tells you which `--ytd` to pass
next. A YFT renders its main body only: wheels and breakable parts are
separate drawables and do not appear. Entries in a drawable dictionary
(`.ydd`) mostly share one name — the file's own — so they are reported and
written out as `0x<hash>` instead, and that hash is what `--entry` takes to
pick one of them. The old `ytd` command still works as an alias for
`textures`, and `--dds` restores its original raw-DDS output.

## Keys

Retail archives are NG-encrypted, so reading them needs keys from your own game
install. Point the tool at the game once and forget about it:

```sh
export GTAV_PATH="<GTA V>"          # or the full path to GTA5.exe
rpf list "<GTA V>/x64a.rpf" "*.ytd"
rpf textures "<GTA V>/x64a.rpf" binoculars.ytd
rpf extract "<GTA V>/x64b.rpf" -o ./nested "*icons.rpf"
rpf screenshot ./nested/levels/gta5/generic/icons.rpf prop_mk_arrow_3d.ydr --views front,iso --grid
```

`--exe <PATH>` does the same thing per-run and overrides the variable. Either
form takes the executable itself or the folder holding it. Keys are read
straight from it each time, which costs under two seconds and writes nothing.

To keep a copy on disk instead, write them out once and use `--keys`, which
takes precedence over `--exe`:

```sh
rpf extract-keys --exe "<GTA V>/GTA5.exe" -o ./keys
rpf list --keys ./keys "<GTA V>/x64a.rpf" "*.ytd"
```

Only the AES key is still stored as plain bytes in GTA5.exe. Newer builds no
longer carry the NG keys or decrypt tables, so those are unwrapped from a
`magic.dat` compiled into the binary, using the AES key found in your
executable.

That `magic.dat` comes from CodeWalker, which generates it by deflating the NG
keys and decrypt tables, encrypting them with the AES key, and masking the
result with a seeded .NET PRNG stream. Unwrapping therefore needs an AES key
from a real game executable, so the keys stay inert without one. The key
material itself is the same across game versions — extracting once is enough.

## Releasing

`rpf-cli` depends on the published `rpf-archive` crate. When developing against
the sibling checkout, add `path = "../rpf-archive-rs"` to the dependency line
temporarily and drop it again before releasing. To cut a release:

1. If a library change is needed, publish `rpf-archive` to crates.io first and
   bump the version this crate pins.
2. Bump this crate's version in `Cargo.toml` and rebuild so `Cargo.lock` follows.
3. Commit, then `gh release create vX.Y.Z --notes ...` — the tag triggers the
   `release` workflow, which builds the Linux and Windows binaries and attaches
   them to that release.

Publishing itself is a manual, deliberate step — nothing here does it for you.

## Acknowledgements

- CodeWalker (<https://github.com/dexyfex/CodeWalker>)
- Swage (<https://github.com/0x1F9F1/Swage>)
- Contributors of <https://gtamods.com/wiki/RPF_archive>
