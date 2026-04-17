# svg2tex-rs

`svg2tex-rs` provides the `svg2tex` command, which converts SVG artwork into PDF literal operators or complete LaTeX source.
It is designed for TeX workflows where SVG graphics should stay as vector content whenever
possible, while still accepting a broad range of real-world SVG files through hybrid rendering.

## Installation

Install directly from GitHub:

```bash
cargo install --git https://github.com/rice8y/svg2tex-rs.git
```

Clone and build locally without installing:

```bash
git clone https://github.com/rice8y/svg2tex-rs.git
cd svg2tex-rs
just build
```

This writes the binary to `target/release/svg2tex`.

Clone and install to `~/.local/bin`:

```bash
git clone https://github.com/rice8y/svg2tex-rs.git
cd svg2tex-rs
just install
```

The default install target is `~/.local/bin/svg2tex`.
Override it with `LOCAL_BIN_DIR=/your/path just install`.

## Quick Start

Emit raw PDF literal operators:

```bash
svg2tex --input drawing.svg --output drawing.literal
```

Emit a standalone LaTeX document:

```bash
svg2tex --input drawing.svg --tex --output drawing.tex
lualatex drawing.tex
```

Emit an embeddable TeX snippet:

```bash
svg2tex --input drawing.svg --tex --tex-format snippet --output drawing-snippet.tex
```

## Output Modes

- Default mode: raw PDF literal operators only
- `--tex`: complete TeX-oriented output
- `--tex-format standalone`: complete cropped document, best default for direct compilation
- `--tex-format article`: complete article document
- `--tex-format snippet`: macro-only TeX fragment for inclusion in another document

## Rendering Model

`svg2tex` tries to preserve SVG content as vector output whenever practical.
When a subtree requires raster-domain processing, it uses hybrid rendering instead of silently
dropping the feature.

Use these flags to control behavior:

- `--strict`: fail instead of using hybrid rendering
- `--fallback-dpi <N>`: control raster fallback resolution
- `--embed-images`: embed raster images referenced by the SVG

## Text and Fonts

Text is flattened into paths during conversion.
For reproducible results across machines, prefer explicit font configuration.

Useful flags:

- `--no-system-fonts`
- `--strict-fonts`
- `--report-fonts`
- `--font-file <PATH>`
- `--font-dir <PATH>`
- `--font-family <NAME>`
- `--serif-family <NAME>`
- `--sans-serif-family <NAME>`
- `--monospace-family <NAME>`

Recommended reproducible setup:

```bash
svg2tex \
  --input drawing.svg \
  --tex \
  --no-system-fonts \
  --strict-fonts \
  --font-file ./fonts/YourFont-Regular.ttf \
  --font-family "Your Font"
```

## TeX Engine Selection

Choose the target backend explicitly when needed:

```bash
svg2tex --input drawing.svg --tex --engine luatex --output drawing.tex
```

Supported engine values:

- `auto`
- `pdftex`
- `luatex`
- `xetex`
- `ptex`
- `uptex`

## Limitations

- Text output depends on the fonts you load into the converter
- Some SVG effects are represented through hybrid raster subtrees rather than pure vector PDF
- Exact pixel-perfect matching with every SVG renderer is not guaranteed, especially for complex text and filter interactions

For a fuller user guide, see [`docs/documentation.pdf`](docs/documentation.pdf).

## License

This project is distributed under the MIT License. See [LICENSE](LICENSE) for details.
