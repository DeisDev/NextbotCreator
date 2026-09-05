# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [NextbotCreator]

## [Unreleased]

### Added

- Dedicated application Settings screen with a gear icon and a back button that preserves the active project and editor.
- Release-only GitHub Actions workflow for stable version tags, publishing one portable Windows x64 ZIP with all bundled tools, an embedded Visual C++ runtime, and a checksum in the release notes after version, code, and package smoke checks.

### Changed

- Moved update preferences, update checks, media downloader updates, and Garry's Mod folder controls into Settings.
- Moved repository and issue tracker links into Settings to keep the toolbar readable at smaller window sizes.
- Made the Project menu easier to recognize with a dropdown arrow and visible border.

## [0.7.0] - 2026-09-05

### Added

- Public YouTube and TikTok audio imports with background downloads, progress, cancellation, and portable source storage.
- Audio playback and reversible trimming for local and downloaded clips, with waveform handles, seeking, zoom, and preview looping.
- Direct image and GIF URL imports for NextBot sprites and custom killfeed icons, with previews and automatic addon conversion.
- Verified portable yt-dlp and Deno downloads in release packaging, bundled FFprobe, and an explicit downloader update action.

### Changed

- Sound entries now retain source paths, optional source URLs, and trim points while loading older projects automatically.
- Asset import validates canonical destination boundaries while preserving relocatable paths.
- App-only packaging accepts `-SkipTools` as an alias for `-SkipFfmpeg`.

## [0.6.0] - 2026-09-04

### Added

- GitHub repository and issue tracker links in the toolbar.
- Background update checks against published GitHub releases, a portable startup-check preference, manual checks, and dismissible new-version notices.
- Compressed portable release ZIPs with SHA-256 checksums, and distinct app-only archives for updating without downloading FFmpeg again.

### Fixed

- Release packaging cleanup now checks directory boundaries before replacing build folders.

## [0.5.1] - 2026-09-04

### Changed

- Replaced the home-screen slogans with a compact Projects header, available-project count, and an Open project action.

## [0.5.0] - 2026-09-04

### Added

- Enemy spotted, chase, enemy lost, melee attack, ranged attack, and landing sound slots with portable asset paths and automatic conversion.
- NextBot and sound search, save/generate/search shortcuts, unsaved-change protection, and undo for NextBot removal.
- An enabled-by-default option to ignore other NextBots and prevent damage between them, including projects saved by earlier versions.

### Changed

- Refined the dark theme, sidebar navigation, editor spacing, project home, and status details.
- Generate addons in the background with a progress indicator so the window stays responsive during conversion.
- Increased Chase preset walking speed from 200 to 210 and running speed from 360 to 380; reapply the preset to update existing settings.

### Fixed

- Removed per-frame FFmpeg probes and repeated addon-link checks from the toolbar and status bar.
- Adding or duplicating NextBots now always chooses a unique entity class name.
- Advanced search opens matching sections and includes property descriptions and section names.

## [0.4.0] - 2026-09-03

### Added

- Optional continuous idle-sound looping with the configured idle delay preserved when disabled.

## [0.3.1] - 2026-09-03

### Changed

- Reduced the Chase preset's movement speed so sprinting players have a chance to escape.

## [0.3.0] - 2026-09-03

### Added

- Static killfeed icon generation from the NextBot sprite or a separate custom image.
- Friendly, aggressive, hostile, and relentless one-hit chase behavior presets.
- Portable recent-project history, including projects opened outside the default projects folder.
- Configurable jump sound effects with automatic Source-compatible audio conversion.

## [0.2.0] - 2026-09-03

### Added

- Top-toolbar button for launching Garry's Mod from the configured installation.

## [0.1.0] - 2026-09-02

### Added

- Portable Windows 10/11 x64 Rust GUI with a minimal dark theme.
- Multi-NextBot addon projects and typed basic/advanced DRGBase settings.
- Static and animated VTF/VMT generation from common images and GIFs.
- Portable FFmpeg-backed audio conversion to Source-compatible PCM WAV.
- Visual, sound, combat, AI, relationship, movement, climbing, weapon, and possession editors.
- Code-free action recipes for every lifecycle hook in the official DRGBase NextBot template.
- Facepunch-style three-file Lua entity generation with version watermarks.
- Server-side admin-only spawn enforcement and automatic client resource registration.
- Steam library auto-detection and verified Garry's Mod directory junction linking.
- NPCs, DrGBase, Entities, and custom spawnmenu placement with admin-only control.
- Relocatable project files and application settings for portable-folder use.

### Fixed

- Long filesystem paths no longer wrap and stretch individual characters in the GUI.
