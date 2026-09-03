# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [NextbotCreator]

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
