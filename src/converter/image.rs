use flate2::{write::ZlibEncoder, Compression};
use std::io::Write;

use super::{ImageResource, PdfConverter, SoftMaskResource};

impl PdfConverter {
    pub(crate) fn process_image(&mut self, img: &usvg::Image, parent_transform: &usvg::Transform) {
        if !self.embed_images {
            eprintln!("Info: Image element skipped (use --embed-images to include)");
            return;
        }

        let abs_transform = img.abs_transform();
        let relative_transform = parent_transform
            .invert()
            .unwrap_or(usvg::Transform::identity())
            .pre_concat(abs_transform);

        match img.kind() {
            usvg::ImageKind::SVG(tree) => {
                self.process_svg_image(tree, &relative_transform);
            }
            _ => {
                let img_name = format!("Img{}", self.resources.get_next_id());
                let image_resource = match self.decode_image_resource(img.kind()) {
                    Some(resource) => resource,
                    None => return,
                };

                eprintln!(
                    "Created Image XObject: {} ({}x{})",
                    img_name, image_resource.width, image_resource.height
                );

                self.resources
                    .images
                    .insert(img_name.clone(), image_resource);

                let rect = img.bounding_box();
                self.pdf_ops.push_str("q ");

                // usvg stores image content in object-local coordinates, so we
                // emit the node transform first and then scale into its box.
                if !super::util::is_identity_transform(&relative_transform) {
                    self.apply_transform(&relative_transform);
                }

                self.pdf_ops.push_str(&format!(
                    "{:.6} 0 0 {:.6} {:.6} {:.6} cm ",
                    rect.width(),
                    rect.height(),
                    rect.left(),
                    rect.top()
                ));
                self.pdf_ops.push_str(&format!("/{} Do ", img_name));
                self.pdf_ops.push_str("Q ");
            }
        }
    }

    fn process_svg_image(&mut self, tree: &usvg::Tree, relative_transform: &usvg::Transform) {
        self.pdf_ops.push_str("q ");

        if !super::util::is_identity_transform(relative_transform) {
            self.apply_transform(relative_transform);
        }

        self.append_rect_path(0.0, 0.0, tree.size().width(), tree.size().height());
        self.pdf_ops.push_str("W n ");

        // Embedded SVG images start a fresh subtree, so children are evaluated
        // relative to the image's own root instead of the outer document.
        for child in tree.root().children() {
            let _ = self.process_node(child, &usvg::Transform::identity());
        }

        self.pdf_ops.push_str("Q ");
    }

    fn decode_image_resource(&mut self, kind: &usvg::ImageKind) -> Option<ImageResource> {
        let bytes = match kind {
            usvg::ImageKind::JPEG(data)
            | usvg::ImageKind::PNG(data)
            | usvg::ImageKind::GIF(data)
            | usvg::ImageKind::WEBP(data) => data.as_ref(),
            usvg::ImageKind::SVG(_) => {
                return None;
            }
        };

        self.decode_image_bytes(bytes)
    }

    pub(crate) fn decode_image_bytes(&mut self, bytes: &[u8]) -> Option<ImageResource> {
        let decoded = image::load_from_memory(bytes).ok().or_else(|| {
            eprintln!("Warning: Failed to decode embedded raster image");
            None
        })?;

        let rgba = decoded.to_rgba8();
        Some(self.image_resource_from_rgba(rgba.width(), rgba.height(), rgba.as_raw()))
    }

    pub(crate) fn image_resource_from_rgba(
        &mut self,
        width: u32,
        height: u32,
        rgba: &[u8],
    ) -> ImageResource {
        let mut rgb = Vec::with_capacity((width * height * 3) as usize);
        let mut alpha = Vec::with_capacity((width * height) as usize);
        let mut has_alpha = false;

        for pixel in rgba.chunks_exact(4) {
            let r = pixel[0];
            let g = pixel[1];
            let b = pixel[2];
            let a = pixel[3];
            rgb.extend_from_slice(&[r, g, b]);
            alpha.push(a);
            if a != 255 {
                has_alpha = true;
            }
        }

        let data = Self::deflate_bytes(&rgb).expect("compressing RGB image data should succeed");

        let smask = if has_alpha {
            let smask_name = format!("SMask{}", self.resources.get_next_id());
            let smask_data = Self::deflate_bytes(&alpha)
                .expect("compressing image alpha channel should succeed");
            Some(SoftMaskResource {
                name: smask_name,
                width,
                height,
                bits_per_component: 8,
                filter: "FlateDecode".to_string(),
                data: smask_data,
            })
        } else {
            None
        };

        ImageResource {
            width,
            height,
            color_space: "DeviceRGB".to_string(),
            bits_per_component: 8,
            filter: "FlateDecode".to_string(),
            data,
            smask,
        }
    }

    fn deflate_bytes(data: &[u8]) -> std::io::Result<Vec<u8>> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(data)?;
        encoder.finish()
    }

    pub(crate) fn tex_image_dict_for_pdftex(
        &self,
        _img_name: &str,
        resource: &ImageResource,
    ) -> String {
        let mut dict = format!(
            "<</Type/XObject/Subtype/Image/Width {}/Height {}/ColorSpace/{}/BitsPerComponent {}/Filter[/ASCIIHexDecode /{}]",
            resource.width,
            resource.height,
            resource.color_space,
            resource.bits_per_component,
            resource.filter
        );

        if let Some(smask) = &resource.smask {
            dict.push_str(&format!("/SMask {}", Self::tex_obj_ref(&smask.name)));
        }

        dict.push_str(">>");
        dict
    }

    pub(crate) fn tex_image_dict_for_lua(
        &self,
        _img_name: &str,
        resource: &ImageResource,
    ) -> String {
        let mut dict = format!(
            "<</Type/XObject/Subtype/Image/Width {}/Height {}/ColorSpace/{}/BitsPerComponent {}/Filter[/ASCIIHexDecode /{}]",
            resource.width,
            resource.height,
            resource.color_space,
            resource.bits_per_component,
            resource.filter
        );

        if let Some(smask) = &resource.smask {
            dict.push_str(&format!("/SMask {}", Self::tex_obj_ref(&smask.name)));
        }

        dict.push_str(">>");
        dict
    }

    pub(crate) fn tex_image_dict_for_pdftex_smask(&self, smask: &SoftMaskResource) -> String {
        format!(
            "<</Type/XObject/Subtype/Image/Width {}/Height {}/ColorSpace/DeviceGray/BitsPerComponent {}/Filter[/ASCIIHexDecode /{}]>>",
            smask.width, smask.height, smask.bits_per_component, smask.filter
        )
    }

    pub(crate) fn tex_image_dict_for_lua_smask(&self, smask: &SoftMaskResource) -> String {
        format!(
            "<</Type/XObject/Subtype/Image/Width {}/Height {}/ColorSpace/DeviceGray/BitsPerComponent {}/Filter[/ASCIIHexDecode /{}]>>",
            smask.width, smask.height, smask.bits_per_component, smask.filter
        )
    }

    pub(crate) fn dvi_image_dict(&self, _img_name: &str, resource: &ImageResource) -> String {
        let mut dict = format!(
            "<</Type/XObject/Subtype/Image/Width {}/Height {}/ColorSpace/{}/BitsPerComponent {}/Filter/{}",
            resource.width,
            resource.height,
            resource.color_space,
            resource.bits_per_component,
            resource.filter
        );

        if let Some(smask) = &resource.smask {
            dict.push_str(&format!("/SMask @{}", smask.name));
        }

        dict.push_str(">>");
        dict
    }

    pub(crate) fn dvi_image_dict_for_smask(&self, smask: &SoftMaskResource) -> String {
        format!(
            "<</Type/XObject/Subtype/Image/Width {}/Height {}/ColorSpace/DeviceGray/BitsPerComponent {}/Filter/{}>>",
            smask.width, smask.height, smask.bits_per_component, smask.filter
        )
    }

    pub(crate) fn ascii_hex_stream(data: &[u8]) -> String {
        let mut out = Self::hex_stream(data);
        out.push('>');
        out
    }

    pub(crate) fn hex_stream(data: &[u8]) -> String {
        data.iter().map(|byte| format!("{:02X}", byte)).collect()
    }
}
