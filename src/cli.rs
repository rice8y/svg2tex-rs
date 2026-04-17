use clap::Parser;
use std::path::PathBuf;

use crate::{TexEngine, TexFormat};

/// Convert SVG files to PDF literals for LaTeX.
#[derive(Parser, Debug, Clone)]
#[command(name = "svg2tex")]
#[command(author, version, about, long_about = None)]
pub struct Args {
    /// Input SVG file path
    #[arg(short, long)]
    pub input: String,

    /// Output file path (if not specified, writes to stdout)
    #[arg(short, long)]
    pub output: Option<String>,

    /// Output a complete LaTeX document instead of just the PDF literal
    #[arg(long, default_value_t = false)]
    pub tex: bool,

    /// TeX document format (standalone, article, snippet)
    #[arg(long, default_value = "standalone")]
    pub tex_format: TexFormat,

    /// Embed raster images found in the SVG
    #[arg(long, default_value_t = false)]
    pub embed_images: bool,

    /// Disable loading system fonts for deterministic text conversion
    #[arg(long, default_value_t = false)]
    pub no_system_fonts: bool,

    /// Fail when text rendering still depends on host font discovery
    #[arg(long, default_value_t = false)]
    pub strict_fonts: bool,

    /// Print font requirements detected in SVG text nodes
    #[arg(long, default_value_t = false)]
    pub report_fonts: bool,

    /// Default font family used when SVG text omits font-family
    #[arg(long)]
    pub font_family: Option<String>,

    /// Default font size used when SVG text omits font-size
    #[arg(long)]
    pub font_size: Option<f32>,

    /// Override the generic serif font family
    #[arg(long)]
    pub serif_family: Option<String>,

    /// Override the generic sans-serif font family
    #[arg(long = "sans-serif-family")]
    pub sans_serif_family: Option<String>,

    /// Override the generic cursive font family
    #[arg(long)]
    pub cursive_family: Option<String>,

    /// Override the generic fantasy font family
    #[arg(long)]
    pub fantasy_family: Option<String>,

    /// Override the generic monospace font family
    #[arg(long)]
    pub monospace_family: Option<String>,

    /// Additional font file to load before parsing SVG text
    #[arg(long = "font-file")]
    pub font_files: Vec<PathBuf>,

    /// Additional font directory to load before parsing SVG text
    #[arg(long = "font-dir")]
    pub font_dirs: Vec<PathBuf>,

    /// Error out instead of rasterizing when unsupported SVG features are detected
    #[arg(long, default_value_t = false)]
    pub strict: bool,

    /// Raster fallback resolution in DPI when unsupported SVG features are present
    #[arg(long, default_value_t = 144.0)]
    pub fallback_dpi: f32,

    /// Target TeX engine (auto, pdftex, luatex, xetex, ptex, uptex)
    #[arg(long, default_value = "auto")]
    pub engine: TexEngine,
}
