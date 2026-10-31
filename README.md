# 🎨 Void Canvas

A terminal-aesthetic ASCII art editor built with Rust, [egui](https://github.com/emilk/egui) and [eframe](https://github.com/emilk/egui/tree/master/crates/eframe).

![Void Canvas screenshot](assets/screenshot.png)

---

## Features

| Category | Details |
|---|---|
| **Tools** | Brush, Eraser, Flood Fill, Rectangle, Ellipse, Line, Border Box, Selection |
| **Layers** | Add / Remove / Duplicate / Merge Down / Reorder / Rename / Lock / Hide |
| **History** | 100-step Undo / Redo for drawing, layer operations and canvas resize |
| **Characters** | 18 built-in palettes (Lines, Blocks, Math, Greek, Arrows, Nerd icons, …) + search |
| **Colors** | Full RGBA picker, primary & secondary color slots, 16 quick-presets |
| **Export** | Native `.vc` format (JSON), plain text `.txt`, and rendered **PNG** |
| **Canvas** | Infinite pan, smooth zoom (0.15× – 8×), optional grid |

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

### Edit

| Shortcut | Action |
|---|---|
| `Ctrl+Z` | Undo |
| `Ctrl+Y` / `Ctrl+Shift+Z` | Redo |
| `Ctrl+C` | Copy selection (also as plain text to the system clipboard) |
| `Ctrl+X` | Cut selection |
| `Ctrl+V` | Paste at cursor (text copied from other apps works too) |
| `Delete` | Delete selection |
| `Escape` | Deselect |
| `X` | Swap primary ↔ secondary |

### Tools

| Key | Tool |
|---|---|
| `B` | Brush |
| `E` | Eraser |
| `F` | Flood Fill |
| `R` | Rectangle |
| `O` | Ellipse |
| `L` | Line |
| `D` | Border Box |
| `S` | Select |
| `+` / `-` | Brush size up / down |

### Canvas

| Input | Action |
|---|---|
| Scroll | Pan |
| `Ctrl` + Scroll | Zoom |
| `Ctrl` + `+` / `-` / `0` | Zoom in / out / reset |
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

**File → Export PNG…** renders the composited canvas at **10 × 18 px per cell** using the bundled FiraCode font.  
Background and foreground RGBA colours are preserved exactly.

---

## Project Structure

```
src/
├── main.rs              Entry point, window setup, font loading
├── app.rs               Main editor state & event loop
├── tests.rs             Model / history tests + headless UI tests (egui_kittest)
├── dev.rs               Debug-only helpers (VOID_CANVAS_SCREENSHOT=out.png)
├── document/
│   ├── cell.rs          Cell & Color types
│   ├── layer.rs         Layer (grid of cells)
│   └── document.rs      Document (layers stack, composite, I/O)
├── history/
│   ├── commands.rs      Command enum
│   └── mod.rs           Undo / Redo stack
├── tools/
│   ├── brush.rs         Brush tool
│   ├── eraser.rs        Eraser tool
│   ├── fill.rs          Flood-fill tool
│   ├── shapes.rs        Rectangle, Ellipse, Line tools
│   ├── border.rs        Border-box tool
│   └── selection.rs     Rectangular selection + move / copy / paste
└── ui/
    ├── canvas.rs         Canvas rendering & input
    ├── char_palette.rs   Character palette panel
    ├── layer_panel.rs    Layers panel
    └── panels.rs         Color & Tool Options panels
```

---

## Dependencies

| Crate | Purpose |
|---|---|
| [egui](https://crates.io/crates/egui) | Immediate-mode GUI (0.35) |
| [eframe](https://crates.io/crates/eframe) | Native window / app shell |
| [serde](https://crates.io/crates/serde) + [serde_json](https://crates.io/crates/serde_json) | Serialisation |
| [rfd](https://crates.io/crates/rfd) | Native file dialogs |
| [image](https://crates.io/crates/image) | PNG encoding |
| [ab_glyph](https://crates.io/crates/ab_glyph) | Font rasterisation for PNG export |
| [egui_kittest](https://crates.io/crates/egui_kittest) | Headless UI tests (dev only) |

---

## License

Modified MIT — see [LICENSE](LICENSE).