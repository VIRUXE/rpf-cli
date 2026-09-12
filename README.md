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
extract       Extract files, with --recursive to descend into nested archives
verify        Verify archive integrity
tree          Display contents as a tree
ytd           Extract textures from a .ytd as DDS files
create        Create an archive from a directory
extract-keys  Write the keys out to disk for reuse with --keys
```

Reading retail game archives needs keys — see below. Archives that are not
encrypted, such as FiveM resource packs, need no setup at all:

```sh
rpf tree resource.rpf
rpf extract resource.rpf -o ./out
```

## Keys

Retail archives are NG-encrypted, so reading them needs keys from your own game
install. Point the tool at the game once and forget about it:

```sh
export GTAV_PATH="<GTA V>"          # or the full path to GTA5.exe
rpf list "<GTA V>/x64a.rpf" "*.ytd"
rpf ytd  "<GTA V>/x64a.rpf" binoculars.ytd
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

## Acknowledgements

- CodeWalker (<https://github.com/dexyfex/CodeWalker>)
- Swage (<https://github.com/0x1F9F1/Swage>)
- Contributors of <https://gtamods.com/wiki/RPF_archive>
