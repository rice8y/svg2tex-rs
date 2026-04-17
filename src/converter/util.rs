use super::PdfConverter;

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
    pub(crate) fn tex_obj_macro(name: &str) -> String {
        format!("\\csname svgobj@{}\\endcsname", name)
    }

    pub(crate) fn tex_obj_ref(name: &str) -> String {
        format!("{} 0 R", Self::tex_obj_macro(name))
    }
}
