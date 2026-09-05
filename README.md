# NextbotCreator

A portable Windows GUI for creating and testing Garry's Mod NextBots based on DRGBase. It converts visual and audio assets, generates structured addon Lua, and links project folders into Garry's Mod for rapid testing.

## Build

Install stable Rust, then run `cargo build --release --locked`. Run `scripts/package.ps1` to create the portable Windows x64 folder and compressed ZIP with FFmpeg and a SHA-256 checksum included. Use `scripts/package.ps1 -SkipFfmpeg` for an app-only ZIP; audio conversion requires the existing `tools/ffmpeg.exe`.

Upload the ZIP and its `.sha256` file to [GitHub Releases](https://github.com/DeisDev/NextbotCreator/releases). Publish stable releases with a matching version tag such as `v0.6.0`. The app checks the latest published stable release in the background at startup; disable this or check manually under **Updates**. To update, close the app and copy the new bundle's contents into your existing portable folder, retaining `settings.json`, `projects`, and `tools`.
