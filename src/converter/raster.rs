use resvg::tiny_skia::{Pixmap, Transform};
use usvg::{Node, NonZeroRect, Tree};

use super::PdfConverter;

impl PdfConverter {
    pub(crate) fn rasterize_tree(&mut self, tree: &Tree) -> Result<(), String> {
        let scale = (self.fallback_dpi / 96.0).max(1.0);
        let width = (self.size.width() * scale).ceil().max(1.0) as u32;
        let height = (self.size.height() * scale).ceil().max(1.0) as u32;

        let mut pixmap = Pixmap::new(width, height)
            .ok_or_else(|| format!("Failed to allocate raster fallback surface: {}x{}", width, height))?;

        let scale_x = width as f32 / self.size.width();
        let scale_y = height as f32 / self.size.height();
        resvg::render(tree, Transform::from_scale(scale_x, scale_y), &mut pixmap.as_mut());

        let png = pixmap
            .encode_png()
            .map_err(|e| format!("Failed to encode raster fallback image: {}", e))?;
        let resource = self
            .decode_image_bytes(&png)
            .ok_or_else(|| "Failed to decode raster fallback image".to_string())?;
        let img_name = format!("Img{}", self.resources.get_next_id());
        self.resources.images.insert(img_name.clone(), resource);

        self.pdf_ops.clear();
        self.pdf_ops.push_str("q ");
        self.pdf_ops
            .push_str(&format!("1 0 0 -1 0 {} cm ", self.size.height()));
        self.pdf_ops.push_str("q ");
        self.pdf_ops.push_str(&format!(
            "{:.6} 0 0 {:.6} 0 0 cm ",
            self.size.width(),
            self.size.height()
        ));
        self.pdf_ops.push_str(&format!("/{} Do ", img_name));
        self.pdf_ops.push_str("Q Q");

        eprintln!(
            "Info: SVG rendered as raster fallback image ({}x{} px at {:.1} dpi)",
            width, height, self.fallback_dpi
        );

        Ok(())
    }

    pub(crate) fn rasterize_node(
        &mut self,
        node: &Node,
        parent_transform: &usvg::Transform,
        reasons: &[String],
    ) -> Result<(), String> {
        let bbox = node
            .abs_layer_bounding_box()
            .ok_or_else(|| "Failed to rasterize unsupported subtree: node has zero size".to_string())?;

        let (png, width, height) = self.render_node_png(node, bbox)?;
        let resource = self
            .decode_image_bytes(&png)
            .ok_or_else(|| "Failed to decode rasterized subtree image".to_string())?;
        let img_name = format!("Img{}", self.resources.get_next_id());
        self.resources.images.insert(img_name.clone(), resource);

        self.pdf_ops.push_str("q ");

        if !super::util::is_identity_transform(parent_transform) {
            let neutralize = parent_transform
                .invert()
                .unwrap_or(usvg::Transform::identity());
            self.apply_transform(&neutralize);
        }

        self.pdf_ops.push_str(&format!(
            "{:.6} 0 0 {:.6} {:.6} {:.6} cm ",
            bbox.width(),
            bbox.height(),
            bbox.left(),
            bbox.top()
        ));
        self.pdf_ops.push_str(&format!("/{} Do ", img_name));
        self.pdf_ops.push_str("Q ");

        eprintln!(
            "Info: Rasterized unsupported subtree ({}) as {}x{} px image at {:.1} dpi",
            reasons.join(", "),
            width,
            height,
            self.fallback_dpi
        );

        Ok(())
    }

    pub(crate) fn rasterize_filtered_group(
        &mut self,
        node: &Node,
        relative_transform: &usvg::Transform,
        reasons: &[String],
    ) -> Result<(), String> {
        let bbox = match node {
            Node::Group(group) => group.abs_layer_bounding_box(),
            _ => {
                return Err(
                    "Filtered raster fallback currently expects a group node".to_string()
                )
            }
        };
        let (png, width, height) = self.render_group_png(node, bbox)?;
        let resource = self
            .decode_image_bytes(&png)
            .ok_or_else(|| "Failed to decode rasterized filter image".to_string())?;
        let img_name = format!("Img{}", self.resources.get_next_id());
        self.resources.images.insert(img_name.clone(), resource);

        self.pdf_ops.push_str("q ");

        if !super::util::is_identity_transform(relative_transform) {
            let neutralize = relative_transform
                .invert()
                .unwrap_or(usvg::Transform::identity());
            self.apply_transform(&neutralize);
        }

        self.pdf_ops.push_str(&format!(
            "{:.6} 0 0 {:.6} {:.6} {:.6} cm ",
            bbox.width(),
            bbox.height(),
            bbox.left(),
            bbox.top()
        ));
        self.pdf_ops.push_str(&format!("/{} Do ", img_name));
        self.pdf_ops.push_str("Q ");

        eprintln!(
            "Info: Rasterized filter group ({}) as {}x{} px image at {:.1} dpi",
            reasons.join(", "),
            width,
            height,
            self.fallback_dpi
        );

        Ok(())
    }

    fn render_node_png(
        &self,
        node: &Node,
        bbox: NonZeroRect,
    ) -> Result<(Vec<u8>, u32, u32), String> {
        let scale = (self.fallback_dpi / 96.0).max(1.0);
        let width = (bbox.width() * scale).ceil().max(1.0) as u32;
        let height = (bbox.height() * scale).ceil().max(1.0) as u32;

        let mut pixmap = Pixmap::new(width, height).ok_or_else(|| {
            format!(
                "Failed to allocate subtree raster surface: {}x{}",
                width, height
            )
        })?;

        let scale_x = width as f32 / bbox.width();
        let scale_y = height as f32 / bbox.height();
        resvg::render_node(node, Transform::from_scale(scale_x, scale_y), &mut pixmap.as_mut())
            .ok_or_else(|| "Failed to render unsupported subtree".to_string())?;

        let png = pixmap
            .encode_png()
            .map_err(|e| format!("Failed to encode subtree raster image: {}", e))?;

        Ok((png, width, height))
    }

    fn render_group_png(
        &self,
        node: &Node,
        bbox: NonZeroRect,
    ) -> Result<(Vec<u8>, u32, u32), String> {
        let scale = (self.fallback_dpi / 96.0).max(1.0);
        let width = (bbox.width() * scale).ceil().max(1.0) as u32;
        let height = (bbox.height() * scale).ceil().max(1.0) as u32;

        let mut pixmap = Pixmap::new(width, height).ok_or_else(|| {
            format!(
                "Failed to allocate filtered group raster surface: {}x{}",
                width, height
            )
        })?;

        let scale_x = width as f32 / bbox.width();
        let scale_y = height as f32 / bbox.height();
        resvg::render_node(node, Transform::from_scale(scale_x, scale_y), &mut pixmap.as_mut())
            .ok_or_else(|| "Failed to render filtered group".to_string())?;

        let png = pixmap
            .encode_png()
            .map_err(|e| format!("Failed to encode filtered group image: {}", e))?;

        Ok((png, width, height))
    }
}
