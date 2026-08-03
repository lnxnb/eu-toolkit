# EU Toolkit

[![Latest release](https://img.shields.io/github/v/release/lnxnb/eu-toolkit)](https://github.com/lnxnb/eu-toolkit/releases/latest)

**[⬇ Download here](https://github.com/lnxnb/eu-toolkit/releases/latest/download/eu-toolkit.exe)** — portable exe, no install needed ([all releases](https://github.com/lnxnb/eu-toolkit/releases))

An all-in-one desktop tool for viewing and editing Europa Universalis IV mods, built with [Tauri 2](https://tauri.app/) (Rust) and [SvelteKit](https://svelte.dev/) (Svelte 5, TypeScript).

Point it at your EU4 installation and (optionally) a mod folder, and you get an interactive world map with the game's map modes — political, religion, culture, trade goods, trade nodes, development, areas/regions, climate, and more — plus editors for just about everything: countries, provinces, rulers, diplomacy, wars, estates, rebels, technology and units, missions, events, decisions, government reforms, trade companies, the HRE, localisation, defines, and beyond.

A few things it cares about:

- **Clean diffs.** Edits are byte-surgical — only the changed span of a file is rewritten, so comments, formatting, and encoding round-trip untouched and your mod's git history stays readable.
- **Total conversions welcome.** All reads go through a virtual file system that layers the mod over the base game (including `replace_path`), so mods like Anbennar work out of the box. Unrecognized keys in game files are preserved, never dropped.
- **View at any date.** The map and editors can derive the world state at any start date, not just 1444.
- **Toolkit workflows.** Project-wide search, a mod-vs-base diff browser, a validation dashboard with jump-to-problem, undo/redo with a pending-edit queue, and one-click export & launch into the game.

## Requirements

- Windows 10/11 (WebView2 is preinstalled on both)
- [Node.js](https://nodejs.org/) 20 or newer
- [Rust](https://rustup.rs/) stable toolchain
- A Europa Universalis IV installation

## Running

Double-click **`run.bat`** (or run it from a terminal). On first run it installs npm dependencies, then builds and starts the app — the initial Rust compile takes a few minutes; later runs are fast. It runs in "stable mode": the running instance is never rebuilt or hot-reloaded out from under you — re-run `run.bat` to pick up source changes.

For development with hot reload instead, use `npm run tauri dev`.

## Building

**`build.bat`** produces a portable, optimized executable at `dist\eu-toolkit.exe`. No installer needed — settings live in `%APPDATA%\com.eutoolkit.app`.

## License

[MIT](LICENSE)
