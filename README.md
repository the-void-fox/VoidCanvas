# 🎨 Void Canvas

A terminal-aesthetic ASCII art editor built with Rust, [egui](https://github.com/emilk/egui) and [eframe](https://github.com/emilk/egui/tree/master/crates/eframe).

![Void Canvas screenshot](assets/screenshot.png)

---

## Features

| Category | Details |
|---|---|
| **Tools** | Select, Brush, Pixel, Shade, Eraser, Flood Fill, Gradient, Eyedropper, Text, Line, Rectangle, Ellipse, Border Box — left button uses the primary slot, right button the secondary; Shift / Alt square or centre shapes |
| **ASCII art** | **Pixel** paints square pixels as half blocks (`▀▄█`, two per cell); **Shade** steps density (`░▒▓█`, ` .:-=+*#%@`); **Gradient** fills the selection with a linear or radial ramp of glyphs (dithered) and colours (blended in OKLab); box lines and borders **join automatically** (`├ ┬ ┼`, double lines too); **symmetry** mirrors strokes and turns glyphs to match |
| **Pictures** | Import PNG / JPEG / GIF / WebP as half blocks, ASCII, shades or braille — colour or mono, dithering, brightness / contrast, live preview |
| **Banners** | Big FIGlet letters (Standard, Small, Slant, Big, Shadow, Mini, Banner) |
| **Paint channels** | Glyph / Fore / Back toggles: paint, fill, erase or pick only the character, only the colour, or only the background |
| **Selection** | Move by dragging or with the arrow keys, glyph-aware Flip / Rotate (`/` ↔ `\`, `┌` ↔ `┐`, `▌` ↔ `▐`, …) |
| **Layers** | Add / Remove / Duplicate / Merge Down / Reorder / Rename / Lock / Hide |
| **History** | 100-step Undo / Redo for everything, with a History panel to jump to any step |
| **Glyphs** | 21 sets (Lines, Box, Blocks, Braille, Sextants, Math, Greek, Arrows, Nerd icons, …), recent glyphs; search by name, code point (`U+2588`) or any typed character |
| **Colors** | Primary & secondary foreground / background slots, live RGBA picker, hex input, recent colours, VGA / xterm 256 / PICO-8 / Game Boy palettes; Replace (Ctrl+H) recolours or swaps glyphs across a layer |
| **Canvas** | Terminal-style 1:2 cells; box-drawing, block and braille characters are drawn as shapes so they join seamlessly at any zoom; fit / 100 % zoom, optional grid |
| **Clipboard** | Copy / cut / paste selections, also as plain text to and from other programs |
| **Files** | Native `.vc` format (JSON), `.txt` import / export, **ANSI** (24-bit colour) and **HTML** export, **PNG** export (opaque or transparent), copy as text / ANSI, recent files, drag & drop, autosave with crash recovery |

---

## Installation

### Pre-built binaries

Download the latest release for your platform from the [Releases](../../releases) page.

### Build from source

**Requirements:** Rust 1.92+ and Cargo.

```bash
git clone https://github.com/fantommripper/VoidCanvas
cd VoidCanvas
cargo build --release
./target/release/void_canvas          # Linux / macOS
./target/release/void_canvas.exe      # Windows
```

---

## Keyboard Shortcuts

### File

| Shortcut | Action |
|---|---|
| `Ctrl+N` | New document |
| `Ctrl+O` | Open `.vc` file |
| `Ctrl+S` | Save |
| `Ctrl+Shift+S` | Save as… |
| Drop a file on the window | Open `.vc` / import `.txt` |

### Edit

| Shortcut | Action |
|---|---|
| `Ctrl+Z` | Undo |
| `Ctrl+Y` / `Ctrl+Shift+Z` | Redo |
| `Ctrl+C` | Copy selection (also as plain text to the system clipboard) |
| `Ctrl+X` | Cut selection |
| `Ctrl+V` | Paste at cursor (text copied from other apps works too) |
| `Ctrl+A` / `Ctrl+D` | Select all / deselect |
| `Ctrl+H` | Replace glyphs / colours |
| `Delete` | Delete selection |
| `Escape` | Deselect |
| `H` / `V` | Flip selection horizontally / vertically |
| Arrows (`Shift`: ×5) | Move the selected block |
| `X` | Swap primary ↔ secondary |

### Tools

| Key | Tool |
|---|---|
| `S` | Select |
| `B` | Brush |
| `E` | Eraser |
| `P` | Pixel (half-block pixels) |
| `G` | Shade (right button: lighter) |
| `F` | Flood Fill |
| `U` | Gradient (right button: reversed) |
| `I` | Eyedropper (or `Alt`+click with any drawing tool) |
| `T` | Text (click, type; `Enter` next line, `Esc` done) |
| `R` | Rectangle |
| `O` | Ellipse |
| `L` | Line |
| `D` | Border Box |
| `[` `]` or `-` `+` | Brush size down / up |
| `Shift` / `Alt` while dragging a shape | Square / straight, from the centre |
| Right button | Draw with the secondary glyph and colours |

### Canvas

| Input | Action |
|---|---|
| Scroll, `Space` + drag | Pan |
| `Ctrl` + Scroll | Zoom |
| `Ctrl` + `+` / `-` | Zoom in / out |
| `Ctrl+0` | Fit to window |
| `Ctrl+1` | Actual size (100 %) |
| Middle-click drag | Pan |

---

## File Format

`.vc` files are plain JSON — human-readable and version-control friendly.  
Every layer, its cells, and their RGBA colours are stored verbatim.

```json
{
  "width": 80,
  "height": 40,
  "layers": [ { "name": "Background", "visible": true, "locked": false, … } ],
  "active_layer": 0
}
```

---

## PNG Export

**File → Export PNG…** renders the composited canvas with the bundled FiraCode font at **10 × 20 px per cell**
(one glyph advance × one line), so block and box-drawing characters join without seams.  
Background and foreground RGBA colours are alpha-blended exactly.
**Export PNG (Transparent)…** leaves empty cells fully transparent instead of black.

---

## Project Structure

```
src/
├── main.rs              Entry point, window setup
├── theme.rs             Colours, fonts and spacing of the "Void" look
├── icons.rs             Material Design Icons from the bundled Nerd Font
├── glyphs.rs            Box-drawing structure, glyph-aware flip / rotate
├── convert.rs           Picture → character art
├── figlet.rs            FIGlet banner fonts
├── export.rs            PNG / ANSI / HTML export
├── tests.rs             Model / history tests + headless UI tests (egui_kittest)
├── dev.rs               Debug-only helpers (VOID_CANVAS_SCREENSHOT=out.png)
├── app/
│   ├── mod.rs           Editor state, input, editing operations
│   ├── chrome.rs        Menu, tool options bar, tool strip, dock, status bar
│   ├── dialogs.rs       New / Resize / About / unsaved-changes / recovery dialogs
│   ├── import.rs        Import Image dialog
│   ├── replace.rs       Replace dialog
│   ├── banner.rs        Insert Banner dialog
│   ├── storage.rs       Settings, recent files, autosave
│   ├── file_dialog.rs   System file dialog, with a built-in chooser as fallback
│   └── file_io.rs       Open, save, export
├── document/
│   ├── cell.rs          Cell & Color types, compositing
│   ├── layer.rs         Layer (grid of cells)
│   └── document.rs      Document (layer stack, composite, I/O)
├── history/
│   ├── commands.rs      Command enum
│   └── mod.rs           Undo / Redo stack
├── tools/               Brush, pixel, shade, eraser, fill, gradient, text, shapes, selection
└── ui/
    ├── canvas.rs         Canvas rendering & input
    ├── glyph_render.rs   Box-drawing / block / braille characters as shapes
    ├── char_palette.rs   Glyph palette & search
    ├── layer_panel.rs    Layers panel
    ├── panels.rs         Colour panel, tool options
    └── widgets.rs        Small custom widgets
```

Open a file directly with `void_canvas path/to/art.vc` — try `examples/demo.vc`.
To re-render the README screenshot offscreen:
`SHOT_DOC=examples/demo.vc SHOT_OUT=assets/screenshot.png cargo test render_window -- --ignored`.

---

## Dependencies

| Crate | Purpose |
|---|---|
| [egui](https://crates.io/crates/egui) | Immediate-mode GUI (0.35) |
| [eframe](https://crates.io/crates/eframe) | Native window / app shell |
| [serde](https://crates.io/crates/serde) + [serde_json](https://crates.io/crates/serde_json) | Serialisation |
| [rfd](https://crates.io/crates/rfd) | Native file dialogs (a built-in chooser takes over when they can't open, e.g. Linux without xdg-desktop-portal) |
| [image](https://crates.io/crates/image) | PNG encoding |
| [ab_glyph](https://crates.io/crates/ab_glyph) | Font rasterisation for PNG export |
| [egui_kittest](https://crates.io/crates/egui_kittest) | Headless UI tests (dev only) |

Banner fonts come from [FIGlet](http://www.figlet.org/) (BSD licence, see `src/assets/figlet/LICENSE`).
Fonts: [Fira Code](https://github.com/tonsky/FiraCode) (Nerd Font build) for the canvas and icons,
[Inter](https://github.com/rsms/inter) for the interface — both under the SIL Open Font License.

---

## License

Modified MIT — see [LICENSE](LICENSE).