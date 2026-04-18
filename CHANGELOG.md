## 0.1.1

### Added

- Extensive internal documentation across modules:

  - converter traversal and transform handling
  - rasterization strategy and transform neutralization
  - paint order and shading behavior
  - TeX wrapper generation and resource expansion
  - supported TeX engines and output formats
  - font validation and raster fallback checks
  - resource interning and deterministic emission
  - CLI entrypoint and crate-level documentation

### Fixed

- Preserve nested `use` transforms in PDF output
- Correct composition of image transforms within group hierarchies
- Ensure proper expansion of page resource dictionaries for pdfTeX and LuaTeX
- Preserve object reference spacing in generated TeX resources

### Changed

- Updated dependency versions

## 0.1.0

### Added

- Initial release of `svg2tex-rs`
- Convert SVG input into raw PDF literal operators
- Generate TeX-friendly output for LaTeX workflows
- Support configurable TeX engines and output formats
- Hybrid raster fallback for unsupported SVG features
- Controls for embedded images, fonts, and strict validation
- Project documentation and Linux release artifact