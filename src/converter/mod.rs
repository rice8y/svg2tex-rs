mod filter;
mod image;
mod latex;
mod paint;
mod raster;
mod resources;
mod util;

use std::collections::HashMap;

use usvg::{Group, Node, Path, Tree};

use crate::validation::{node_requires_raster, node_unsupported_features};
use crate::{TexEngine, TexFormat};

pub(crate) struct PdfConverter {
    pub(crate) size: usvg::Size,
    pub(crate) resources: PdfResources,
    pub(crate) pdf_ops: String,
    pub(crate) ctx: PdfContext,
    pub(crate) embed_images: bool,
    pub(crate) fallback_dpi: f32,
    pub(crate) engine: TexEngine,
    pub(crate) tex_format: TexFormat,
}

pub(crate) struct PdfResources {
    pub(crate) ext_gstates: HashMap<String, ExtGStateResource>,
    pub(crate) functions: HashMap<String, FunctionResource>,
    pub(crate) images: HashMap<String, ImageResource>,
    pub(crate) shadings: HashMap<String, ShadingResource>,
    pub(crate) forms: HashMap<String, FormResource>,
    pub(crate) patterns: HashMap<String, PatternResource>,
    pub(crate) next_id: usize,
}

pub(crate) struct ImageResource {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) color_space: String,
    pub(crate) bits_per_component: u8,
    pub(crate) filter: String,
    pub(crate) data: Vec<u8>,
    pub(crate) smask: Option<SoftMaskResource>,
}

pub(crate) struct SoftMaskResource {
    pub(crate) name: String,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bits_per_component: u8,
    pub(crate) filter: String,
    pub(crate) data: Vec<u8>,
}

pub(crate) struct ShadingResource {
    pub(crate) name: String,
    pub(crate) dict: String,
}

pub(crate) struct FunctionResource {
    pub(crate) name: String,
    pub(crate) pdf_dict: String,
    pub(crate) dvi_dict: String,
}

pub(crate) struct ExtGStateResource {
    pub(crate) name: String,
    pub(crate) pdf_dict: String,
    pub(crate) dvi_dict: String,
}

pub(crate) struct FormResource {
    pub(crate) name: String,
    pub(crate) pdf_dict: String,
    pub(crate) dvi_dict: String,
    pub(crate) stream: Vec<u8>,
}

pub(crate) struct PatternResource {
    pub(crate) name: String,
    pub(crate) pdf_dict: String,
    pub(crate) dvi_dict: String,
    pub(crate) stream: Vec<u8>,
}

pub(crate) struct PdfContext {
    pub(crate) current_point: Option<(f32, f32)>,
    pub(crate) subpath_start: Option<(f32, f32)>,
}

impl PdfConverter {
    pub(crate) fn new(
        size: usvg::Size,
        embed_images: bool,
        fallback_dpi: f32,
        engine: TexEngine,
        tex_format: TexFormat,
    ) -> Self {
        Self {
            size,
            resources: PdfResources::new(),
            pdf_ops: String::new(),
            ctx: PdfContext::new(),
            embed_images,
            fallback_dpi,
            engine,
            tex_format,
        }
    }

    pub(crate) fn convert(&mut self, tree: &Tree) -> Result<(), String> {
        self.pdf_ops.push_str("q ");
        self.pdf_ops
            .push_str(&format!("1 0 0 -1 0 {} cm ", self.size.height()));

        eprintln!("SVG size: {}x{}", self.size.width(), self.size.height());

        for node in tree.root().children() {
            self.process_node(node, &usvg::Transform::identity())?;
        }

        self.pdf_ops.push_str("Q");
        Ok(())
    }

    pub(crate) fn generate_pdf_literal(&self) -> String {
        self.pdf_ops.clone()
    }

    pub(crate) fn process_node(
        &mut self,
        node: &Node,
        parent_transform: &usvg::Transform,
    ) -> Result<(), String> {
        if node_requires_raster(node, self.embed_images) {
            let reasons = node_unsupported_features(node, self.embed_images);
            return self.rasterize_node(node, parent_transform, &reasons);
        }

        match node {
            Node::Path(path) => self.process_path(path, parent_transform),
            Node::Group(group) => self.process_group(Some(node), group, parent_transform),
            Node::Image(img) => {
                self.process_image(img, parent_transform);
                Ok(())
            }
            Node::Text(text) => self.process_text(text, parent_transform),
        }
    }

    fn process_group(
        &mut self,
        group_node: Option<&Node>,
        group: &Group,
        parent_transform: &usvg::Transform,
    ) -> Result<(), String> {
        let abs_transform = group.abs_transform();
        let relative_transform = parent_transform
            .invert()
            .unwrap_or(usvg::Transform::identity())
            .post_concat(abs_transform);

        let has_transform = !self::util::is_identity_transform(&relative_transform);
        let opacity = group.opacity().get();
        let has_opacity = opacity < 1.0;
        let has_clip_path = group.clip_path().is_some();
        let has_mask = group.mask().is_some();
        let has_blend_mode = !matches!(group.blend_mode(), usvg::BlendMode::Normal);
        let has_filters = !group.filters().is_empty();

        let needs_state =
            has_transform || has_opacity || has_clip_path || has_mask || has_blend_mode || has_filters;

        if needs_state {
            self.pdf_ops.push_str("q ");

            if has_transform {
                self.apply_transform(&relative_transform);
            }

            if has_opacity || has_blend_mode {
                self.apply_graphics_state(opacity, group.blend_mode());
            }

            if let Some(clip_path) = group.clip_path() {
                self.process_clip_path(clip_path)?;
            }

            if let Some(mask) = group.mask() {
                let gs_name = self.ensure_mask_ext_gstate(mask)?;
                self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
            }
        }

        if has_filters {
            self.process_filter_group(group_node, group, &relative_transform)?;
        } else {
            for child in group.children() {
                self.process_node(child, &abs_transform)?;
            }
        }

        if needs_state {
            self.pdf_ops.push_str("Q ");
        }

        Ok(())
    }

    fn process_path(
        &mut self,
        path: &Path,
        parent_transform: &usvg::Transform,
    ) -> Result<(), String> {
        let abs_transform = path.abs_transform();
        let relative_transform = parent_transform
            .invert()
            .unwrap_or(usvg::Transform::identity())
            .post_concat(abs_transform);

        self.pdf_ops.push_str("q ");

        if !self::util::is_identity_transform(&relative_transform) {
            self.apply_transform(&relative_transform);
        }

        self.render_path_components(path)?;

        self.pdf_ops.push_str("Q ");
        Ok(())
    }

    fn process_clip_path(&mut self, clip_path: &usvg::ClipPath) -> Result<(), String> {
        if self.clip_path_uses_soft_mask(clip_path) {
            let gs_name = self.ensure_clip_path_ext_gstate(clip_path)?;
            self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
            return Ok(());
        }

        self.emit_vector_clip_path(clip_path);
        Ok(())
    }

    fn emit_vector_clip_path(&mut self, clip_path: &usvg::ClipPath) {
        let transform = clip_path.transform();
        let has_transform = !self::util::is_identity_transform(&transform);

        if has_transform {
            self.pdf_ops.push_str("q ");
            self.apply_transform(&transform);
        }

        for node in clip_path.root().children() {
            self.append_clip_node(node);
        }

        self.pdf_ops.push_str("W n ");

        if has_transform {
            self.pdf_ops.push_str("Q ");
        }
    }

    fn clip_path_uses_soft_mask(&self, clip_path: &usvg::ClipPath) -> bool {
        clip_path
            .clip_path()
            .map(|nested| self.clip_path_uses_soft_mask(nested))
            .unwrap_or(false)
            || clip_path
                .root()
                .children()
                .iter()
                .any(Self::clip_node_contains_image)
    }

    fn clip_node_contains_image(node: &Node) -> bool {
        match node {
            Node::Image(_) => true,
            Node::Group(group) => group.children().iter().any(Self::clip_node_contains_image),
            Node::Text(text) => text
                .flattened()
                .children()
                .iter()
                .any(Self::clip_node_contains_image),
            Node::Path(_) => false,
        }
    }

    fn ensure_clip_path_ext_gstate(
        &mut self,
        clip_path: &usvg::ClipPath,
    ) -> Result<String, String> {
        let form_name = self.ensure_clip_path_form(clip_path)?;
        Ok(self.ensure_soft_mask_ext_gstate(&form_name, "Alpha"))
    }

    fn ensure_clip_path_form(&mut self, clip_path: &usvg::ClipPath) -> Result<String, String> {
        let key = format!(
            "clip-path/{}/{}/{}/{}",
            clip_path.id(),
            Self::pdf_matrix(clip_path.transform()),
            clip_path.root().id(),
            clip_path.root().children().len()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return Ok(resource.name.clone());
        }

        let stream = self.capture_stream(|converter| converter.render_clip_path_stream(clip_path))?;
        let pdf_resources = self.inline_pdf_resource_dict(true);
        let dvi_resources = self.inline_dvi_resource_dict(true);
        let pdf_dict = format!(
            "<</Type/XObject/Subtype/Form/BBox [0 0 {:.6} {:.6}] /Group <</S /Transparency /CS /DeviceRGB>> /Resources {} /Filter [/ASCIIHexDecode]>>",
            self.size.width(),
            self.size.height(),
            if pdf_resources.is_empty() { "<<>>".to_string() } else { pdf_resources }
        );
        let dvi_dict = format!(
            "<</Type/XObject/Subtype/Form/BBox [0 0 {:.6} {:.6}] /Group <</S /Transparency /CS /DeviceRGB>> /Resources {} /Filter /ASCIIHexDecode>>",
            self.size.width(),
            self.size.height(),
            if dvi_resources.is_empty() { "<<>>".to_string() } else { dvi_resources }
        );

        Ok(self.ensure_form(key, pdf_dict, dvi_dict, stream))
    }

    fn render_clip_path_stream(&mut self, clip_path: &usvg::ClipPath) -> Result<(), String> {
        self.pdf_ops
            .push_str(&format!("q 1 0 0 -1 0 {:.6} cm ", self.size.height()));

        if let Some(nested) = clip_path.clip_path() {
            if self.clip_path_uses_soft_mask(nested) {
                let gs_name = self.ensure_clip_path_ext_gstate(nested)?;
                self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
            } else {
                self.emit_vector_clip_path(nested);
            }
        }

        if !self::util::is_identity_transform(&clip_path.transform()) {
            self.apply_transform(&clip_path.transform());
        }

        let saved_embed_images = self.embed_images;
        self.embed_images = true;
        let result = self.process_group(None, clip_path.root(), &usvg::Transform::identity());
        self.embed_images = saved_embed_images;

        result?;
        self.pdf_ops.push_str("Q");
        Ok(())
    }

    fn append_clip_node(&mut self, node: &Node) {
        match node {
            Node::Path(path) => self.convert_path_data(path),
            Node::Group(group) => {
                for child in group.children() {
                    self.append_clip_node(child);
                }
            }
            Node::Text(text) => {
                for child in text.flattened().children() {
                    self.append_clip_node(child);
                }
            }
            Node::Image(_) => {}
        }
    }

    fn process_text(
        &mut self,
        text: &usvg::Text,
        parent_transform: &usvg::Transform,
    ) -> Result<(), String> {
        eprintln!("Processing text node as flattened paths");
        self.process_group(None, text.flattened(), parent_transform)
    }

    fn convert_path_data(&mut self, path: &Path) {
        self.append_tiny_skia_path(path.data());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fill_opacity_uses_extgstate() {
        let mut converter =
            PdfConverter::new(
                usvg::Size::from_wh(10.0, 10.0).unwrap(),
                false,
                144.0,
                TexEngine::PdfTeX,
                TexFormat::Standalone,
            );
        let color = usvg::Color::new_rgb(255, 0, 0);
        converter.apply_paint(&usvg::Paint::Color(color), 0.5, true);

        assert!(converter.pdf_ops.contains("rg"));
        assert!(converter.pdf_ops.contains(" gs "));
        assert!(!converter.pdf_ops.contains(" ca "));
    }

    #[test]
    fn page_resources_merge_xobjects_once() {
        let mut resources = PdfResources::new();
        resources.ext_gstates.insert(
            "/ca 0.500000".to_string(),
            ExtGStateResource {
                name: "GS1".to_string(),
                pdf_dict: "<</Type/ExtGState /ca 0.500000>>".to_string(),
                dvi_dict: "<</Type/ExtGState /ca 0.500000>>".to_string(),
            },
        );
        resources.images.insert(
            "Img2".to_string(),
            ImageResource {
                width: 1,
                height: 1,
                color_space: "DeviceRGB".to_string(),
                bits_per_component: 8,
                filter: "FlateDecode".to_string(),
                data: vec![0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
                smask: None,
            },
        );
        resources.images.insert(
            "Img3".to_string(),
            ImageResource {
                width: 1,
                height: 1,
                color_space: "DeviceRGB".to_string(),
                bits_per_component: 8,
                filter: "FlateDecode".to_string(),
                data: vec![0x78, 0x9C, 0x03, 0x00, 0x00, 0x00, 0x00, 0x01],
                smask: None,
            },
        );

        let converter = PdfConverter {
            size: usvg::Size::from_wh(10.0, 10.0).unwrap(),
            resources,
            pdf_ops: String::new(),
            ctx: PdfContext::new(),
            embed_images: false,
            fallback_dpi: 144.0,
            engine: TexEngine::PdfTeX,
            tex_format: TexFormat::Standalone,
        };

        let resources = converter.build_pdf_page_resources();
        assert_eq!(resources.matches("/XObject<<").count(), 1);
        assert!(resources.contains("/Img2 \\csname svgobj@Img2\\endcsname 0 R"));
        assert!(resources.contains("/Img3 \\csname svgobj@Img3\\endcsname 0 R"));
    }

    #[test]
    fn apply_transform_uses_pdf_matrix_order() {
        let mut converter =
            PdfConverter::new(
                usvg::Size::from_wh(10.0, 10.0).unwrap(),
                false,
                144.0,
                TexEngine::PdfTeX,
                TexFormat::Standalone,
            );
        let transform = usvg::Transform {
            sx: 1.0,
            kx: 2.0,
            ky: 3.0,
            sy: 4.0,
            tx: 5.0,
            ty: 6.0,
        };

        converter.apply_transform(&transform);

        assert_eq!(converter.pdf_ops, "1.000000 3.000000 2.000000 4.000000 5.000000 6.000000 cm ");
        assert_eq!(PdfConverter::pdf_matrix(transform), "1.000000 3.000000 2.000000 4.000000 5.000000 6.000000");
    }
}
