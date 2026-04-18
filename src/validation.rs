use std::collections::HashSet;

use usvg::fontdb;
use usvg::{FontFamily, Node, Paint, Tree};

use crate::converter::PdfConverter;

#[derive(Debug, Default)]
pub(crate) struct TreeAnalysis {
    pub(crate) has_text_nodes: bool,
    pub(crate) unsupported_features: Vec<String>,
    pub(crate) text_font_requirements: Vec<TextFontRequirement>,
}

/// Describes one distinct font request observed while traversing SVG text.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TextFontRequirement {
    pub families: Vec<String>,
    pub style: String,
    pub stretch: String,
    pub weight: u16,
}

impl TextFontRequirement {
    /// Formats a stable, human-readable description for diagnostics.
    pub fn summary(&self) -> String {
        format!(
            "families=[{}], style={}, stretch={}, weight={}",
            self.families.join(", "),
            self.style,
            self.stretch,
            self.weight
        )
    }

    fn named_families(&self) -> impl Iterator<Item = &str> {
        self.families
            .iter()
            .filter_map(|family| family.strip_prefix("named:"))
    }
}

impl TreeAnalysis {
    /// Returns named font families that were requested by the SVG text but are
    /// not present in the currently loaded font database.
    pub(crate) fn missing_named_font_families(
        &self,
        fontdb: &std::sync::Arc<fontdb::Database>,
    ) -> Vec<String> {
        let loaded = fontdb
            .faces()
            .flat_map(|face| face.families.iter().map(|(name, _)| name.to_lowercase()))
            .collect::<HashSet<_>>();
        let mut missing = self
            .text_font_requirements
            .iter()
            .flat_map(|requirement| requirement.named_families())
            .filter(|family| !loaded.contains(&family.to_lowercase()))
            .map(str::to_string)
            .collect::<Vec<_>>();
        missing.sort();
        missing.dedup();
        missing
    }
}

/// Collects conversion-relevant metadata from the parsed SVG tree.
pub(crate) fn analyze_tree(tree: &Tree, embed_images: bool) -> TreeAnalysis {
    let mut analysis = TreeAnalysis::default();

    for node in tree.root().children() {
        analyze_subtree(node, embed_images, &mut analysis);
    }

    analysis
}

/// Checks whether the converter must fall back to rasterization for this node.
pub(crate) fn node_requires_raster(node: &Node, embed_images: bool) -> bool {
    !node_unsupported_features(node, embed_images).is_empty()
}

/// Lists the unsupported features that force raster fallback for this node.
pub(crate) fn node_unsupported_features(node: &Node, embed_images: bool) -> Vec<String> {
    let mut features = Vec::new();
    collect_direct_features(node, embed_images, &mut features);
    features
}

fn analyze_subtree(node: &Node, embed_images: bool, analysis: &mut TreeAnalysis) {
    for feature in node_unsupported_features(node, embed_images) {
        push_feature(analysis, &feature);
    }

    match node {
        Node::Path(path) => {
            let _ = path;
        }
        Node::Group(group) => {
            for child in group.children() {
                analyze_subtree(child, embed_images, analysis);
            }
        }
        Node::Image(_) => {}
        Node::Text(text) => {
            analysis.has_text_nodes = true;
            collect_text_font_requirements(text, analysis);
            // usvg exposes flattened text as a synthetic group of paths, so we
            // keep traversing to detect unsupported features inside text runs.
            for child in text.flattened().children() {
                analyze_subtree(child, embed_images, analysis);
            }
        }
    }
}

fn collect_text_font_requirements(text: &usvg::Text, analysis: &mut TreeAnalysis) {
    for chunk in text.chunks() {
        for span in chunk.spans() {
            let requirement = TextFontRequirement {
                families: span
                    .font()
                    .families()
                    .iter()
                    .map(font_family_name)
                    .collect(),
                style: format!("{:?}", span.font().style()).to_lowercase(),
                stretch: format!("{:?}", span.font().stretch()).to_lowercase(),
                weight: span.font().weight(),
            };
            if !analysis.text_font_requirements.contains(&requirement) {
                analysis.text_font_requirements.push(requirement);
            }
        }
    }
}

fn font_family_name(family: &FontFamily) -> String {
    match family {
        FontFamily::Named(name) => format!("named:{name}"),
        FontFamily::Serif => "generic:serif".to_string(),
        FontFamily::SansSerif => "generic:sans-serif".to_string(),
        FontFamily::Cursive => "generic:cursive".to_string(),
        FontFamily::Fantasy => "generic:fantasy".to_string(),
        FontFamily::Monospace => "generic:monospace".to_string(),
    }
}

fn collect_direct_features(node: &Node, embed_images: bool, features: &mut Vec<String>) {
    match node {
        Node::Path(path) => {
            if let Some(fill) = path.fill() {
                collect_paint_features(fill.paint(), "fill", features);
            }
            if let Some(stroke) = path.stroke() {
                collect_paint_features(stroke.paint(), "stroke", features);
            }
        }
        Node::Group(group) => {
            if !group.filters().is_empty() && !PdfConverter::filters_are_supported(group.filters())
            {
                push_feature_name(features, "filters");
            }

            if let Some(clip_path) = group.clip_path() {
                for clip_node in clip_path.root().children() {
                    collect_clip_features(clip_node, features);
                }
            }
        }
        Node::Image(image) => {
            let _ = (embed_images, image);
        }
        Node::Text(_) => {}
    }
}

fn collect_clip_features(node: &Node, features: &mut Vec<String>) {
    match node {
        Node::Image(_) => {
            let _ = features;
        }
        Node::Group(group) => {
            for child in group.children() {
                collect_clip_features(child, features);
            }
        }
        Node::Text(text) => {
            for child in text.flattened().children() {
                collect_clip_features(child, features);
            }
        }
        Node::Path(_) => {}
    }
}

fn collect_paint_features(paint: &Paint, context: &str, features: &mut Vec<String>) {
    match paint {
        Paint::Color(_) => {}
        Paint::LinearGradient(gradient) => {
            if !PdfConverter::gradient_is_natively_supported(
                gradient.stops(),
                gradient.spread_method(),
            ) {
                push_feature_name(features, &format!("{context} linear gradients"));
            }
        }
        Paint::RadialGradient(gradient) => {
            if !PdfConverter::gradient_is_natively_supported(
                gradient.stops(),
                gradient.spread_method(),
            ) {
                push_feature_name(features, &format!("{context} radial gradients"));
            }
        }
        Paint::Pattern(_) => {}
    }
}

fn push_feature(analysis: &mut TreeAnalysis, feature: &str) {
    if !analysis
        .unsupported_features
        .iter()
        .any(|existing| existing == feature)
    {
        analysis.unsupported_features.push(feature.to_string());
    }
}

fn push_feature_name(features: &mut Vec<String>, feature: &str) {
    if !features.iter().any(|existing| existing == feature) {
        features.push(feature.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use usvg::{Options, Tree};

    fn parse(svg: &str) -> Tree {
        Tree::from_data(svg.as_bytes(), &Options::default()).unwrap()
    }

    #[test]
    fn accepts_pad_linear_gradient_with_uniform_opacity() {
        let tree = parse(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<defs><linearGradient id="g"><stop offset="0" stop-color="#000"/><stop offset="1" stop-color="#fff"/></linearGradient></defs>
<rect width="10" height="10" fill="url(#g)"/>
</svg>"##,
        );

        let analysis = analyze_tree(&tree, false);
        assert!(analysis.unsupported_features.is_empty());
    }

    #[test]
    fn supports_filters_via_native_or_raster_path() {
        let tree = parse(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<defs><filter id="f"><feGaussianBlur stdDeviation="1"/></filter></defs>
<g filter="url(#f)"><rect width="10" height="10" fill="#000"/></g>
</svg>"##,
        );

        assert!(!node_requires_raster(&tree.root().children()[0], false));
    }

    #[test]
    fn accepts_native_filter_graph() {
        let tree = parse(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<defs>
  <filter id="f" x="0" y="0" width="10" height="10" filterUnits="userSpaceOnUse">
    <feOffset in="SourceGraphic" dx="1" dy="0" result="offset"/>
    <feComposite in="offset" in2="SourceGraphic" operator="out" result="shadow-shape"/>
    <feFlood flood-color="#00f" flood-opacity="0.5" result="flood"/>
    <feComposite in="flood" in2="shadow-shape" operator="in" result="shadow"/>
    <feMerge>
      <feMergeNode in="shadow"/>
      <feMergeNode in="SourceGraphic"/>
    </feMerge>
  </filter>
</defs>
<g filter="url(#f)"><rect width="8" height="8" fill="#f00"/></g>
</svg>"##,
        );

        assert!(!node_requires_raster(&tree.root().children()[0], false));
    }

    #[test]
    fn accepts_repeat_and_reflect_gradients() {
        let tree = parse(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<defs>
  <linearGradient id="l" x1="0" y1="0" x2="2" y2="0" spreadMethod="repeat">
    <stop offset="0" stop-color="#000"/>
    <stop offset="1" stop-color="#fff"/>
  </linearGradient>
  <radialGradient id="r" cx="3" cy="3" r="2" spreadMethod="reflect">
    <stop offset="0" stop-color="#fff"/>
    <stop offset="1" stop-color="#000"/>
  </radialGradient>
</defs>
<rect width="5" height="5" fill="url(#l)"/>
<rect x="5" width="5" height="5" fill="url(#r)"/>
</svg>"##,
        );

        let analysis = analyze_tree(&tree, false);
        assert!(analysis.unsupported_features.is_empty());
    }

    #[test]
    fn accepts_embedded_svg_images_when_embedding() {
        let tree = parse(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<image width="5" height="5" href="data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='5' height='5'%3E%3Crect width='5' height='5' fill='%23f00'/%3E%3C/svg%3E"/>
</svg>"##,
        );

        assert!(!node_requires_raster(&tree.root().children()[0], true));
    }

    #[test]
    fn mask_is_supported_natively() {
        let tree = parse(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<defs>
  <mask id="m" maskUnits="userSpaceOnUse">
    <rect width="10" height="10" fill="white"/>
  </mask>
</defs>
<g mask="url(#m)"><rect width="10" height="10" fill="#000"/></g>
</svg>"##,
        );

        assert!(!node_requires_raster(&tree.root().children()[0], false));
    }

    #[test]
    fn supported_rect_does_not_require_raster() {
        let tree = parse(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<rect x="1" y="2" width="3" height="4" fill="#f00"/>
</svg>"##,
        );

        assert!(!node_requires_raster(&tree.root().children()[0], false));
    }

    #[test]
    fn variable_opacity_gradient_is_supported_natively() {
        let tree = parse(
            r##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10">
<defs><linearGradient id="g"><stop offset="0" stop-color="#000" stop-opacity="1"/><stop offset="1" stop-color="#fff" stop-opacity="0.5"/></linearGradient></defs>
<rect width="10" height="10" fill="url(#g)"/>
</svg>"##,
        );

        assert!(!node_requires_raster(&tree.root().children()[0], false));
    }
}
