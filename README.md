# NextbotCreator

A portable Windows GUI for creating and testing Garry's Mod NextBots based on DRGBase. It converts visual and audio assets, generates structured addon Lua, and links project folders into Garry's Mod for rapid testing.

## Build

Install stable Rust, then run `cargo build --release --locked`. Run `scripts/package.ps1` to create the portable Windows x64 folder and compressed ZIP with FFmpeg, FFprobe, yt-dlp, Deno, and a SHA-256 checksum included. Use `scripts/package.ps1 -SkipTools` (or `-SkipFfmpeg`) for an app-only ZIP that reuses your existing `tools` folder.

Use **Paste link** in a sound slot for public YouTube/TikTok videos, or **Paste image URL** for direct images and GIFs. **Preview / trim** supports playback, waveform seeking, and reversible start/end edits for local and downloaded audio. Sources stay inside the project; conversion and trimming are applied automatically when generating. Use **Updates > Update downloader** if video imports stop working.

To release, update the version in `Cargo.toml` and `Cargo.lock`, add a dated entry in `CHANGELOG.md`, and push a matching stable tag such as `v0.7.0`. The release workflow checks, builds, and smoke tests the Windows x64 bundle, then publishes one ZIP to [GitHub Releases](https://github.com/DeisDev/NextbotCreator/releases) with the changelog entry and SHA-256 checksum in the release notes. It runs only on stable version tags.

The app checks the latest published stable release in the background at startup; disable this or check manually under **Updates**. To update, close the app and copy the new bundle's contents into your existing portable folder, retaining `settings.json`, `projects`, and `tools`.
