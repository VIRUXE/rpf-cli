@echo off
cargo build --release --target x86_64-unknown-linux-musl
cargo build --release --target x86_64-pc-windows-msvc

@REM Move both binaries to /dist folder
if not exist dist (mkdir dist)

move target\x86_64-unknown-linux-musl\release\rpf dist\rpf
move target\x86_64-pc-windows-msvc\release\rpf.exe dist\rpf.exe