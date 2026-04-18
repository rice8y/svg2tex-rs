use super::PdfConverter;

/// Compares transforms with a small epsilon so numerically noisy identity
/// matrices do not emit redundant PDF `cm` operators.
pub(crate) fn is_identity_transform(transform: &usvg::Transform) -> bool {
    const EPSILON: f32 = 1e-6;

    (transform.sx - 1.0).abs() < EPSILON
        && transform.kx.abs() < EPSILON
        && transform.ky.abs() < EPSILON
        && (transform.sy - 1.0).abs() < EPSILON
        && transform.tx.abs() < EPSILON
        && transform.ty.abs() < EPSILON
}

impl PdfConverter {
    /// Returns the TeX macro name that stores a reserved PDF object number.
    pub(crate) fn tex_obj_macro(name: &str) -> String {
        format!("\\csname svgobj@{}\\endcsname", name)
    }

    /// Returns a TeX-side indirect object reference.
    ///
    /// The explicit `\\space` keeps expansion safe inside `\\expanded` resource
    /// dictionaries for pdfTeX and LuaTeX.
    pub(crate) fn tex_obj_ref(name: &str) -> String {
        format!("{}\\space 0 R", Self::tex_obj_macro(name))
    }
}
