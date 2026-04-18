//! Library entry points for converting SVG input into PDF operators or TeX.
//!
//! The crate keeps CLI parsing separate from conversion so tests and other
//! callers can run the same pipeline through [`render_output`].

mod cli;
mod converter;
mod preprocess;
mod tex_engine;
mod tex_format;
mod validation;

pub use cli::Args;
pub use tex_engine::TexEngine;
pub use tex_format::TexFormat;

use converter::PdfConverter;
use preprocess::preprocess_svg;
use std::fs::File;
use std::io::Write;
use usvg::{Options, Tree};
use validation::{analyze_tree, TextFontRequirement};

/// Renders an SVG input into either raw PDF operators or TeX source.
///
/// This is the main library entry point used by both the CLI and tests.
pub fn render_output(args: &Args) -> Result<String, String> {
    if !args.fallback_dpi.is_finite() || args.fallback_dpi <= 0.0 {
        return Err(format!(
            "Invalid --fallback-dpi value '{}': expected a positive number.",
            args.fallback_dpi
        ));
    }

    if let Some(font_size) = args.font_size {
        if !font_size.is_finite() || font_size <= 0.0 {
            return Err(format!(
                "Invalid --font-size value '{}': expected a positive number.",
                font_size
            ));
        }
    }

    let mut opt = Options::default();
    if let Some(font_family) = &args.font_family {
        opt.font_family = font_family.clone();
    }
    if let Some(font_size) = args.font_size {
        opt.font_size = font_size;
    }
    if let Some(family) = &args.serif_family {
        opt.fontdb_mut().set_serif_family(family.clone());
    }
    if let Some(family) = &args.sans_serif_family {
        opt.fontdb_mut().set_sans_serif_family(family.clone());
    }
    if let Some(family) = &args.cursive_family {
        opt.fontdb_mut().set_cursive_family(family.clone());
    }
    if let Some(family) = &args.fantasy_family {
        opt.fontdb_mut().set_fantasy_family(family.clone());
    }
    if let Some(family) = &args.monospace_family {
        opt.fontdb_mut().set_monospace_family(family.clone());
    }
    if !args.no_system_fonts {
        opt.fontdb_mut().load_system_fonts();
    }

    for font_file in &args.font_files {
        opt.fontdb_mut()
            .load_font_file(font_file)
            .map_err(|e| format!("Error loading font file '{}': {}", font_file.display(), e))?;
    }

    for font_dir in &args.font_dirs {
        opt.fontdb_mut().load_fonts_dir(font_dir);
    }

    let svg_data = std::fs::read(&args.input)
        .map_err(|e| format!("Error reading input file '{}': {}", args.input, e))?;
    let svg_data = preprocess_svg(&svg_data);

    let tree = Tree::from_data(&svg_data, &opt).map_err(|e| format!("Error parsing SVG: {}", e))?;
    let analysis = analyze_tree(&tree, args.embed_images);

    if analysis.has_text_nodes {
        if args.report_fonts {
            report_text_fonts(&analysis.text_font_requirements);
        }

        if !args.no_system_fonts {
            let requested = analysis
                .text_font_requirements
                .iter()
                .map(TextFontRequirement::summary)
                .collect::<Vec<_>>();
            eprintln!(
                "Warning: Text conversion currently depends on system fonts. Requested text fonts: {}. \
Use --no-system-fonts together with --font-file/--font-dir and explicit default font families for reproducible output.",
                if requested.is_empty() {
                    "(none)".to_string()
                } else {
                    requested.join("; ")
                }
            );
        }

        let missing_named_families = analysis.missing_named_font_families(tree.fontdb());
        if args.strict_fonts {
            if !args.no_system_fonts {
                return Err(
                    "--strict-fonts requires --no-system-fonts so text rendering does not depend on host font discovery."
                        .to_string(),
                );
            }

            if !missing_named_families.is_empty() {
                return Err(format!(
                    "Missing named fonts required by SVG text: {}. Load them with --font-file/--font-dir or change the default font families.",
                    missing_named_families.join(", ")
                ));
            }
        } else if !missing_named_families.is_empty() {
            eprintln!(
                "Warning: Some named fonts requested by SVG text were not found in the loaded font database: {}. Fallback shaping may change layout.",
                missing_named_families.join(", ")
            );
        }
    }

    let size = tree.size();
    let mut converter = PdfConverter::new(
        size,
        args.embed_images,
        args.fallback_dpi,
        args.engine,
        args.tex_format,
    );
    // Strict mode rejects unsupported features before conversion so the caller
    // can decide whether a raster fallback is acceptable.
    if !analysis.unsupported_features.is_empty() && args.strict {
        return Err(format!(
            "Unsupported SVG features detected: {}. Re-run without --strict to rasterize only the unsupported subtrees.",
            analysis.unsupported_features.join(", ")
        ));
    }

    if !analysis.unsupported_features.is_empty() {
        eprintln!(
            "Info: Hybrid rendering enabled for unsupported SVG features: {}",
            analysis.unsupported_features.join(", ")
        );
    }

    match converter.convert(&tree) {
        Ok(()) => {}
        Err(err) if !analysis.unsupported_features.is_empty() => {
            eprintln!(
                "Info: Hybrid rendering failed ({}); falling back to full-document rasterization.",
                err
            );
            converter.rasterize_tree(&tree)?;
        }
        Err(err) => return Err(err),
    }

    Ok(if args.tex {
        converter.generate_latex()
    } else {
        converter.generate_pdf_literal()
    })
}

/// Runs the full conversion pipeline and writes the result to stdout or a file.
pub fn run(args: Args) -> Result<(), String> {
    let output_content = render_output(&args)?;

    if let Some(output_path) = args.output {
        let mut file = File::create(&output_path)
            .map_err(|e| format!("Error creating output file '{}': {}", output_path, e))?;

        file.write_all(output_content.as_bytes())
            .map_err(|e| format!("Error writing to output file: {}", e))?;

        eprintln!("Output written to: {}", output_path);
    } else {
        print!("{}", output_content);
    }

    Ok(())
}

fn report_text_fonts(requirements: &[TextFontRequirement]) {
    if requirements.is_empty() {
        eprintln!("Info: No text font requirements were detected.");
        return;
    }

    eprintln!("Text font requirements:");
    for requirement in requirements {
        eprintln!("  - {}", requirement.summary());
    }
}
