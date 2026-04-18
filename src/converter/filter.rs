//! Filter support for the converter.
//!
//! The implementation prefers native PDF constructs when a filter graph can be
//! represented directly; otherwise the filtered subtree is rasterized.

use std::collections::HashMap;

use usvg::filter::{
    ColorMatrixKind, ComponentTransfer, CompositeOperator, Filter, Input, Kind, Primitive,
    TransferFunction,
};
use usvg::{BlendMode, Color, Group, Node, NonZeroRect};

use super::{PdfContext, PdfConverter};

#[derive(Clone)]
struct FilterValue {
    form_name: String,
    alpha_form_name: String,
    bbox: NonZeroRect,
}

impl PdfConverter {
    pub(crate) fn filters_are_natively_supported(filters: &[std::sync::Arc<Filter>]) -> bool {
        filters
            .iter()
            .all(|filter| Self::filter_is_natively_supported(filter))
    }

    pub(crate) fn filters_are_supported(_filters: &[std::sync::Arc<Filter>]) -> bool {
        true
    }

    fn filter_is_natively_supported(filter: &Filter) -> bool {
        filter
            .primitives()
            .iter()
            .all(Self::primitive_is_natively_supported)
    }

    fn primitive_is_natively_supported(primitive: &Primitive) -> bool {
        match primitive.kind() {
            Kind::Blend(_)
            | Kind::Image(_)
            | Kind::Flood(_)
            | Kind::Merge(_)
            | Kind::Offset(_)
            | Kind::Tile(_) => true,
            Kind::ColorMatrix(color_matrix) => {
                matches!(color_matrix.kind(), ColorMatrixKind::LuminanceToAlpha)
            }
            Kind::ComponentTransfer(transfer) => {
                Self::component_transfer_is_natively_supported(transfer)
            }
            Kind::Composite(composite) => {
                !matches!(composite.operator(), CompositeOperator::Arithmetic { .. })
            }
            _ => false,
        }
    }

    pub(crate) fn process_filter_group(
        &mut self,
        group_node: Option<&Node>,
        group: &Group,
        relative_transform: &usvg::Transform,
    ) -> Result<(), String> {
        // Unsupported filter primitives fall back to a rasterized snapshot of
        // the group so the surrounding document can still stay vector-based.
        if !Self::filters_are_natively_supported(group.filters()) {
            let reasons = Self::filter_feature_names(group.filters());
            let node = group_node.ok_or_else(|| {
                "Rasterizing filtered text subtrees is not supported in this path".to_string()
            })?;
            return self.rasterize_filtered_group(node, relative_transform, &reasons);
        }

        self.process_filter_group_native(group)
    }

    fn process_filter_group_native(&mut self, group: &Group) -> Result<(), String> {
        let mut current = self.ensure_source_graphic_form(group)?;

        for filter in group.filters() {
            current = self.apply_filter(filter, &current)?;
        }

        self.pdf_ops
            .push_str(&format!("/{} Do ", current.form_name));
        Ok(())
    }

    fn filter_feature_names(filters: &[std::sync::Arc<Filter>]) -> Vec<String> {
        let mut features = Vec::new();

        for filter in filters {
            for primitive in filter.primitives() {
                let name = match primitive.kind() {
                    Kind::Blend(_) => "feBlend",
                    Kind::ColorMatrix(_) => "feColorMatrix",
                    Kind::ComponentTransfer(_) => "feComponentTransfer",
                    Kind::Composite(_) => "feComposite",
                    Kind::ConvolveMatrix(_) => "feConvolveMatrix",
                    Kind::DiffuseLighting(_) => "feDiffuseLighting",
                    Kind::DisplacementMap(_) => "feDisplacementMap",
                    Kind::DropShadow(_) => "feDropShadow",
                    Kind::Flood(_) => "feFlood",
                    Kind::GaussianBlur(_) => "feGaussianBlur",
                    Kind::Image(_) => "feImage",
                    Kind::Merge(_) => "feMerge",
                    Kind::Morphology(_) => "feMorphology",
                    Kind::Offset(_) => "feOffset",
                    Kind::SpecularLighting(_) => "feSpecularLighting",
                    Kind::Tile(_) => "feTile",
                    Kind::Turbulence(_) => "feTurbulence",
                };

                if !features.iter().any(|existing| existing == name) {
                    features.push(name.to_string());
                }
            }
        }

        features
    }

    fn apply_filter(
        &mut self,
        filter: &Filter,
        source_graphic: &FilterValue,
    ) -> Result<FilterValue, String> {
        let mut results = HashMap::new();
        let mut last = source_graphic.clone();

        for primitive in filter.primitives() {
            // SVG filter inputs can reference earlier `result` names, so we
            // retain each intermediate output while walking the primitive list.
            let value = self.apply_filter_primitive(primitive, source_graphic, &results)?;
            if !primitive.result().is_empty() {
                results.insert(primitive.result().to_string(), value.clone());
            }
            last = value;
        }

        self.ensure_clipped_filter_value(&last, filter.rect())
    }

    fn apply_filter_primitive(
        &mut self,
        primitive: &Primitive,
        source_graphic: &FilterValue,
        results: &HashMap<String, FilterValue>,
    ) -> Result<FilterValue, String> {
        match primitive.kind() {
            Kind::Flood(flood) => {
                let form_name =
                    self.ensure_flood_form(primitive.rect(), flood.color(), flood.opacity().get());
                Ok(FilterValue {
                    form_name: form_name.clone(),
                    alpha_form_name: form_name,
                    bbox: primitive.rect(),
                })
            }
            Kind::Offset(offset) => {
                let input = self.resolve_filter_input(offset.input(), source_graphic, results)?;
                let form_name = self.ensure_offset_form(
                    &input.form_name,
                    primitive.rect(),
                    offset.dx(),
                    offset.dy(),
                );
                Ok(FilterValue {
                    form_name: form_name.clone(),
                    alpha_form_name: form_name,
                    bbox: primitive.rect(),
                })
            }
            Kind::Image(image) => {
                let form_name = self.ensure_filter_image_form(image.root())?;
                let alpha_form_name = self.ensure_source_alpha_form(&form_name);
                Ok(FilterValue {
                    form_name,
                    alpha_form_name,
                    bbox: primitive.rect(),
                })
            }
            Kind::ColorMatrix(color_matrix) => match color_matrix.kind() {
                ColorMatrixKind::LuminanceToAlpha => {
                    let input =
                        self.resolve_filter_input(color_matrix.input(), source_graphic, results)?;
                    let form_name =
                        self.ensure_luminance_to_alpha_form(primitive.rect(), &input.form_name);
                    Ok(FilterValue {
                        form_name: form_name.clone(),
                        alpha_form_name: form_name,
                        bbox: primitive.rect(),
                    })
                }
                other => Err(format!(
                    "Unsupported native feColorMatrix variant: {other:?}"
                )),
            },
            Kind::ComponentTransfer(transfer) => {
                let input = self.resolve_filter_input(transfer.input(), source_graphic, results)?;
                self.ensure_component_transfer_filter_value(primitive.rect(), &input, transfer)
            }
            Kind::Blend(blend) => {
                let source = self.resolve_filter_input(blend.input1(), source_graphic, results)?;
                let backdrop =
                    self.resolve_filter_input(blend.input2(), source_graphic, results)?;
                let form_name = self.ensure_blend_form(
                    primitive.rect(),
                    &backdrop.form_name,
                    &source.form_name,
                    blend.mode(),
                );
                Ok(FilterValue {
                    form_name: form_name.clone(),
                    alpha_form_name: form_name,
                    bbox: primitive.rect(),
                })
            }
            Kind::Merge(merge) => {
                let inputs = merge
                    .inputs()
                    .iter()
                    .map(|input| {
                        self.resolve_filter_input(input, source_graphic, results)
                            .map(|value| value.form_name)
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let form_name = self.ensure_merge_form(primitive.rect(), &inputs);
                Ok(FilterValue {
                    form_name: form_name.clone(),
                    alpha_form_name: form_name,
                    bbox: primitive.rect(),
                })
            }
            Kind::Tile(tile) => {
                let input = self.resolve_filter_input(tile.input(), source_graphic, results)?;
                let form_name =
                    self.ensure_tile_form(&input.form_name, input.bbox, primitive.rect());
                let alpha_form_name =
                    self.ensure_tile_form(&input.alpha_form_name, input.bbox, primitive.rect());
                Ok(FilterValue {
                    form_name,
                    alpha_form_name,
                    bbox: primitive.rect(),
                })
            }
            Kind::Composite(composite) => {
                let source =
                    self.resolve_filter_input(composite.input1(), source_graphic, results)?;
                let backdrop =
                    self.resolve_filter_input(composite.input2(), source_graphic, results)?;
                self.ensure_composite_filter_value(
                    primitive.rect(),
                    &source,
                    &backdrop,
                    composite.operator(),
                )
            }
            other => Err(format!("Unsupported native filter primitive: {other:?}")),
        }
    }

    fn component_transfer_is_natively_supported(transfer: &ComponentTransfer) -> bool {
        Self::transfer_function_is_identity(transfer.func_a())
    }

    fn transfer_function_is_identity(func: &TransferFunction) -> bool {
        match func {
            TransferFunction::Identity => true,
            TransferFunction::Table(values) | TransferFunction::Discrete(values) => {
                values.is_empty()
            }
            TransferFunction::Linear { slope, intercept } => {
                (*slope - 1.0).abs() < 1e-6 && intercept.abs() < 1e-6
            }
            TransferFunction::Gamma {
                amplitude,
                exponent,
                offset,
            } => {
                (*amplitude - 1.0).abs() < 1e-6
                    && (*exponent - 1.0).abs() < 1e-6
                    && offset.abs() < 1e-6
            }
        }
    }

    fn ensure_component_transfer_filter_value(
        &mut self,
        rect: NonZeroRect,
        input: &FilterValue,
        transfer: &ComponentTransfer,
    ) -> Result<FilterValue, String> {
        if !Self::component_transfer_is_natively_supported(transfer) {
            return Err(
                "feComponentTransfer with alpha transfer still requires raster fallback"
                    .to_string(),
            );
        }

        let form_name = if Self::transfer_function_is_identity(transfer.func_r())
            && Self::transfer_function_is_identity(transfer.func_g())
            && Self::transfer_function_is_identity(transfer.func_b())
        {
            self.ensure_clipped_form(&input.form_name, rect)
        } else {
            self.ensure_component_transfer_rgb_form(rect, &input.form_name, transfer)
        };
        let alpha_form_name = self.ensure_clipped_form(&input.alpha_form_name, rect);

        Ok(FilterValue {
            form_name,
            alpha_form_name,
            bbox: rect,
        })
    }

    fn resolve_filter_input(
        &mut self,
        input: &Input,
        source_graphic: &FilterValue,
        results: &HashMap<String, FilterValue>,
    ) -> Result<FilterValue, String> {
        match input {
            Input::SourceGraphic => Ok(source_graphic.clone()),
            Input::SourceAlpha => {
                let form_name = self.ensure_source_alpha_form(&source_graphic.alpha_form_name);
                Ok(FilterValue {
                    form_name,
                    alpha_form_name: source_graphic.alpha_form_name.clone(),
                    bbox: source_graphic.bbox,
                })
            }
            Input::Reference(name) => results.get(name).cloned().ok_or_else(|| {
                format!("Filter input '{name}' does not reference an earlier result")
            }),
        }
    }

    fn ensure_composite_filter_value(
        &mut self,
        rect: NonZeroRect,
        source: &FilterValue,
        backdrop: &FilterValue,
        operator: CompositeOperator,
    ) -> Result<FilterValue, String> {
        match operator {
            CompositeOperator::Over => {
                let form_name = self.ensure_merge_form(
                    rect,
                    &[backdrop.form_name.clone(), source.form_name.clone()],
                );
                Ok(FilterValue {
                    form_name: form_name.clone(),
                    alpha_form_name: form_name,
                    bbox: rect,
                })
            }
            CompositeOperator::In => {
                let form_name = self.ensure_masked_form(
                    rect,
                    &source.form_name,
                    &backdrop.alpha_form_name,
                    false,
                );
                Ok(FilterValue {
                    form_name: form_name.clone(),
                    alpha_form_name: form_name,
                    bbox: rect,
                })
            }
            CompositeOperator::Out => {
                let form_name = self.ensure_masked_form(
                    rect,
                    &source.form_name,
                    &backdrop.alpha_form_name,
                    true,
                );
                Ok(FilterValue {
                    form_name: form_name.clone(),
                    alpha_form_name: form_name,
                    bbox: rect,
                })
            }
            CompositeOperator::Atop => {
                let source_in = self.ensure_masked_form(
                    rect,
                    &source.form_name,
                    &backdrop.alpha_form_name,
                    false,
                );
                let backdrop_out = self.ensure_masked_form(
                    rect,
                    &backdrop.form_name,
                    &source.alpha_form_name,
                    true,
                );
                let form_name = self.ensure_merge_form(rect, &[backdrop_out, source_in]);
                Ok(FilterValue {
                    form_name: form_name.clone(),
                    alpha_form_name: form_name,
                    bbox: rect,
                })
            }
            CompositeOperator::Xor => {
                let source_out = self.ensure_masked_form(
                    rect,
                    &source.form_name,
                    &backdrop.alpha_form_name,
                    true,
                );
                let backdrop_out = self.ensure_masked_form(
                    rect,
                    &backdrop.form_name,
                    &source.alpha_form_name,
                    true,
                );
                let form_name = self.ensure_merge_form(rect, &[backdrop_out, source_out]);
                Ok(FilterValue {
                    form_name: form_name.clone(),
                    alpha_form_name: form_name,
                    bbox: rect,
                })
            }
            CompositeOperator::Arithmetic { .. } => {
                Err("feComposite operator=\"arithmetic\" is not supported natively".to_string())
            }
        }
    }

    fn ensure_source_graphic_form(&mut self, group: &Group) -> Result<FilterValue, String> {
        let key = format!(
            "filter/source/{}/{}/{}",
            group.id(),
            Self::pdf_matrix(group.abs_transform()),
            group.children().len()
        );
        let form_name = if let Some(resource) = self.resources.forms.get(&key) {
            resource.name.clone()
        } else {
            let stream = self.capture_stream(|converter| {
                // Filter inputs are defined in the group's absolute coordinate
                // space, so we re-enter child traversal with the group's
                // absolute transform instead of the caller's local transform.
                for child in group.children() {
                    converter.process_node(child, &group.abs_transform())?;
                }
                Ok(())
            })?;
            self.ensure_transparency_form(key, self.full_page_bbox(), stream)
        };

        Ok(FilterValue {
            form_name: form_name.clone(),
            alpha_form_name: form_name,
            bbox: group.abs_layer_bounding_box(),
        })
    }

    fn ensure_source_alpha_form(&mut self, alpha_source_form: &str) -> String {
        let key = format!("filter/source-alpha/{alpha_source_form}");
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let black = self.ensure_flood_form(self.full_page_bbox(), Color::new_rgb(0, 0, 0), 1.0);
        let stream = format!(
            "q /{} gs /{} Do Q",
            self.ensure_soft_mask_ext_gstate(alpha_source_form, "Alpha"),
            black
        )
        .into_bytes();

        self.ensure_transparency_form(key, self.full_page_bbox(), stream)
    }

    fn ensure_filter_image_form(&mut self, root: &Group) -> Result<String, String> {
        let key = format!(
            "filter/image/{}/{}/{}",
            root.id(),
            Self::pdf_matrix(root.abs_transform()),
            root.children().len()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return Ok(resource.name.clone());
        }

        let stream = self.capture_stream(|converter| {
            let saved_embed_images = converter.embed_images;
            converter.embed_images = true;
            let result = converter.process_group(None, root, &usvg::Transform::identity());
            converter.embed_images = saved_embed_images;
            result?;
            Ok(())
        })?;

        Ok(self.ensure_transparency_form(key, self.full_page_bbox(), stream))
    }

    fn ensure_clipped_filter_value(
        &mut self,
        value: &FilterValue,
        rect: NonZeroRect,
    ) -> Result<FilterValue, String> {
        let form_name = self.ensure_clipped_form(&value.form_name, rect);
        let alpha_form_name = if value.alpha_form_name == value.form_name {
            form_name.clone()
        } else {
            self.ensure_clipped_form(&value.alpha_form_name, rect)
        };

        Ok(FilterValue {
            form_name,
            alpha_form_name,
            bbox: rect,
        })
    }

    fn ensure_component_transfer_rgb_form(
        &mut self,
        rect: NonZeroRect,
        input_form: &str,
        transfer: &ComponentTransfer,
    ) -> String {
        let key = format!(
            "filter/component-transfer/{input_form}/{}/{}/{}/{:.6}/{:.6}/{:.6}/{:.6}",
            Self::transfer_function_key(transfer.func_r()),
            Self::transfer_function_key(transfer.func_g()),
            Self::transfer_function_key(transfer.func_b()),
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let gs_name = self.ensure_component_transfer_ext_gstate(transfer);
        let stream = format!(
            "q {} /{} gs /{} Do Q",
            Self::clip_rect_ops(rect),
            gs_name,
            input_form
        )
        .into_bytes();

        self.ensure_transparency_form(key, rect, stream)
    }

    fn ensure_component_transfer_ext_gstate(&mut self, transfer: &ComponentTransfer) -> String {
        let key = format!(
            "filter/component-transfer-gstate/{}/{}/{}",
            Self::transfer_function_key(transfer.func_r()),
            Self::transfer_function_key(transfer.func_g()),
            Self::transfer_function_key(transfer.func_b())
        );
        let pdf_dict = format!(
            "<</Type/ExtGState /TR2 [{} {} {}]>>",
            Self::transfer_function_pdf_dict(transfer.func_r()),
            Self::transfer_function_pdf_dict(transfer.func_g()),
            Self::transfer_function_pdf_dict(transfer.func_b())
        );
        let dvi_dict = format!(
            "<</Type/ExtGState /TR2 [{} {} {}]>>",
            Self::transfer_function_dvi_dict(transfer.func_r()),
            Self::transfer_function_dvi_dict(transfer.func_g()),
            Self::transfer_function_dvi_dict(transfer.func_b())
        );
        self.ensure_ext_gstate_with_dicts(key, pdf_dict, dvi_dict)
    }

    fn ensure_luminance_to_alpha_form(&mut self, rect: NonZeroRect, input_form: &str) -> String {
        let key = format!(
            "filter/color-matrix/luminance-to-alpha/{input_form}/{:.6}/{:.6}/{:.6}/{:.6}",
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let gs_name = self.ensure_soft_mask_ext_gstate(input_form, "Luminosity");
        let black = self.ensure_flood_form(rect, Color::new_rgb(0, 0, 0), 1.0);
        let stream = format!(
            "q {} /{} gs /{} Do Q",
            Self::clip_rect_ops(rect),
            gs_name,
            black
        )
        .into_bytes();

        self.ensure_transparency_form(key, rect, stream)
    }

    fn ensure_flood_form(&mut self, rect: NonZeroRect, color: Color, opacity: f32) -> String {
        let key = format!(
            "filter/flood/{}/{}/{}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}",
            color.red,
            color.green,
            color.blue,
            opacity,
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height(),
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let mut stream = String::new();
        stream.push_str("q ");
        if opacity < 1.0 {
            if let Some(gs_name) = self.ensure_ext_gstate(&[format!("/ca {:.6}", opacity)]) {
                stream.push_str(&format!("/{} gs ", gs_name));
            }
        }
        stream.push_str(&format!(
            "{:.6} {:.6} {:.6} rg ",
            color.red as f32 / 255.0,
            color.green as f32 / 255.0,
            color.blue as f32 / 255.0
        ));
        stream.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} re f Q",
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height()
        ));

        self.ensure_transparency_form(key, rect, stream.into_bytes())
    }

    fn ensure_offset_form(
        &mut self,
        input_form: &str,
        rect: NonZeroRect,
        dx: f32,
        dy: f32,
    ) -> String {
        let key = format!(
            "filter/offset/{input_form}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}",
            dx,
            dy,
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let stream = format!(
            "q {} {:.6} {:.6} cm /{} Do Q",
            Self::clip_rect_ops(rect),
            dx,
            dy,
            input_form
        )
        .into_bytes();

        self.ensure_transparency_form(key, rect, stream)
    }

    fn ensure_blend_form(
        &mut self,
        rect: NonZeroRect,
        backdrop_form: &str,
        source_form: &str,
        blend_mode: BlendMode,
    ) -> String {
        let key = format!(
            "filter/blend/{backdrop_form}/{source_form}/{:?}/{:.6}/{:.6}/{:.6}/{:.6}",
            blend_mode,
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let gs_name = self
            .ensure_ext_gstate(&[format!("/BM/{}", Self::blend_mode_name(blend_mode))])
            .expect("blend mode extgstate should exist");
        let stream = format!(
            "q {} /{} Do /{} gs /{} Do Q",
            Self::clip_rect_ops(rect),
            backdrop_form,
            gs_name,
            source_form
        )
        .into_bytes();

        self.ensure_transparency_form(key, rect, stream)
    }

    fn ensure_merge_form(&mut self, rect: NonZeroRect, input_forms: &[String]) -> String {
        let key = format!(
            "filter/merge/{}/{:.6}/{:.6}/{:.6}/{:.6}",
            input_forms.join(","),
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let mut stream = format!("q {} ", Self::clip_rect_ops(rect));
        for form in input_forms {
            stream.push_str(&format!("/{} Do ", form));
        }
        stream.push('Q');

        self.ensure_transparency_form(key, rect, stream.into_bytes())
    }

    fn ensure_tile_form(
        &mut self,
        input_form: &str,
        input_bbox: NonZeroRect,
        rect: NonZeroRect,
    ) -> String {
        let key = format!(
            "filter/tile/{input_form}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}",
            input_bbox.x(),
            input_bbox.y(),
            input_bbox.width(),
            input_bbox.height(),
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let cell_w = input_bbox.width();
        let cell_h = input_bbox.height();
        let rect_right = rect.x() + rect.width();
        let rect_bottom = rect.y() + rect.height();
        let start_x = input_bbox.x() + ((rect.x() - input_bbox.x()) / cell_w).floor() * cell_w;
        let start_y = input_bbox.y() + ((rect.y() - input_bbox.y()) / cell_h).floor() * cell_h;

        let mut stream = format!("q {} ", Self::clip_rect_ops(rect));
        let mut y = start_y;
        while y < rect_bottom {
            let mut x = start_x;
            while x < rect_right {
                stream.push_str(&format!(
                    "q 1 0 0 1 {:.6} {:.6} cm /{} Do Q ",
                    x - input_bbox.x(),
                    y - input_bbox.y(),
                    input_form
                ));
                x += cell_w;
            }
            y += cell_h;
        }
        stream.push('Q');

        self.ensure_transparency_form(key, rect, stream.into_bytes())
    }

    fn ensure_masked_form(
        &mut self,
        rect: NonZeroRect,
        input_form: &str,
        mask_form: &str,
        invert: bool,
    ) -> String {
        let key = format!(
            "filter/masked/{input_form}/{mask_form}/{invert}/{:.6}/{:.6}/{:.6}/{:.6}",
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let transfer_name = invert.then(|| self.ensure_invert_transfer_function());
        let gs_name = self.ensure_soft_mask_ext_gstate_with_transfer(
            mask_form,
            "Alpha",
            transfer_name.as_deref(),
        );
        let stream = format!(
            "q {} /{} gs /{} Do Q",
            Self::clip_rect_ops(rect),
            gs_name,
            input_form
        )
        .into_bytes();

        self.ensure_transparency_form(key, rect, stream)
    }

    fn ensure_clipped_form(&mut self, input_form: &str, rect: NonZeroRect) -> String {
        let key = format!(
            "filter/clipped/{input_form}/{:.6}/{:.6}/{:.6}/{:.6}",
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let stream = format!("q {} /{} Do Q", Self::clip_rect_ops(rect), input_form).into_bytes();
        self.ensure_transparency_form(key, rect, stream)
    }

    fn ensure_invert_transfer_function(&mut self) -> String {
        self.ensure_function(
            "filter/transfer/invert".to_string(),
            "<< /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >>".to_string(),
            "<< /FunctionType 2 /Domain [0 1] /C0 [1] /C1 [0] /N 1 >>".to_string(),
        )
    }

    fn transfer_function_key(func: &TransferFunction) -> String {
        match func {
            TransferFunction::Identity => "identity".to_string(),
            TransferFunction::Table(values) => {
                format!("table({})", Self::transfer_values_key(values))
            }
            TransferFunction::Discrete(values) => {
                format!("discrete({})", Self::transfer_values_key(values))
            }
            TransferFunction::Linear { slope, intercept } => {
                format!("linear({slope:.6},{intercept:.6})")
            }
            TransferFunction::Gamma {
                amplitude,
                exponent,
                offset,
            } => format!("gamma({amplitude:.6},{exponent:.6},{offset:.6})"),
        }
    }

    fn transfer_values_key(values: &[f32]) -> String {
        values
            .iter()
            .map(|value| format!("{value:.6}"))
            .collect::<Vec<_>>()
            .join(",")
    }

    fn transfer_function_pdf_dict(func: &TransferFunction) -> String {
        Self::transfer_function_dict(func, false)
    }

    fn transfer_function_dvi_dict(func: &TransferFunction) -> String {
        Self::transfer_function_dict(func, true)
    }

    fn transfer_function_dict(func: &TransferFunction, _dvi: bool) -> String {
        match func {
            TransferFunction::Identity => {
                "<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>".to_string()
            }
            TransferFunction::Linear { slope, intercept } => format!(
                "<< /FunctionType 2 /Domain [0 1] /C0 [{:.6}] /C1 [{:.6}] /N 1 >>",
                intercept,
                slope + intercept
            ),
            TransferFunction::Gamma {
                amplitude,
                exponent,
                offset,
            } => format!(
                "<< /FunctionType 2 /Domain [0 1] /C0 [{:.6}] /C1 [{:.6}] /N {:.6} >>",
                offset,
                amplitude + offset,
                exponent
            ),
            TransferFunction::Table(values) => Self::sampled_transfer_function_dict(values, false),
            TransferFunction::Discrete(values) => {
                Self::sampled_transfer_function_dict(values, true)
            }
        }
    }

    fn sampled_transfer_function_dict(values: &[f32], discrete: bool) -> String {
        if values.is_empty() {
            return "<< /FunctionType 2 /Domain [0 1] /C0 [0] /C1 [1] /N 1 >>".to_string();
        }
        if values.len() == 1 {
            return format!(
                "<< /FunctionType 2 /Domain [0 1] /C0 [{0:.6}] /C1 [{0:.6}] /N 1 >>",
                values[0]
            );
        }

        let segment_count = if discrete {
            values.len()
        } else {
            values.len() - 1
        };
        let functions = (0..segment_count)
            .map(|index| {
                let start = values[index];
                let end = if discrete { start } else { values[index + 1] };
                format!(
                    "<< /FunctionType 2 /Domain [0 1] /C0 [{:.6}] /C1 [{:.6}] /N 1 >>",
                    start, end
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        let divisor = segment_count as f32;
        let bounds = (1..segment_count)
            .map(|index| format!("{:.6}", index as f32 / divisor))
            .collect::<Vec<_>>()
            .join(" ");
        let encode = (0..segment_count)
            .map(|_| "0 1".to_string())
            .collect::<Vec<_>>()
            .join(" ");

        format!(
            "<< /FunctionType 3 /Domain [0 1] /Functions [{}] /Bounds [{}] /Encode [{}] >>",
            functions, bounds, encode
        )
    }

    pub(crate) fn ensure_transparency_form(
        &mut self,
        key: String,
        bbox: NonZeroRect,
        stream: Vec<u8>,
    ) -> String {
        let pdf_resources = self.inline_pdf_resource_dict(true);
        let dvi_resources = self.inline_dvi_resource_dict(true);
        let pdf_dict = format!(
            "<</Type/XObject/Subtype/Form/BBox [{}] /Group <</S /Transparency /CS /DeviceRGB>> /Resources {} /Filter [/ASCIIHexDecode]>>",
            Self::pdf_bbox(bbox),
            if pdf_resources.is_empty() { "<<>>".to_string() } else { pdf_resources }
        );
        let dvi_dict = format!(
            "<</Type/XObject/Subtype/Form/BBox [{}] /Group <</S /Transparency /CS /DeviceRGB>> /Resources {} /Filter /ASCIIHexDecode>>",
            Self::pdf_bbox(bbox),
            if dvi_resources.is_empty() { "<<>>".to_string() } else { dvi_resources }
        );

        self.ensure_form(key, pdf_dict, dvi_dict, stream)
    }

    pub(crate) fn capture_stream<F>(&mut self, f: F) -> Result<Vec<u8>, String>
    where
        F: FnOnce(&mut Self) -> Result<(), String>,
    {
        let saved_ops = std::mem::take(&mut self.pdf_ops);
        let saved_ctx = std::mem::replace(&mut self.ctx, PdfContext::new());

        let result = f(self);
        let stream = self.pdf_ops.clone().into_bytes();

        self.pdf_ops = saved_ops;
        self.ctx = saved_ctx;

        result.map(|_| stream)
    }

    pub(crate) fn full_page_bbox(&self) -> NonZeroRect {
        NonZeroRect::from_xywh(0.0, 0.0, self.size.width(), self.size.height())
            .expect("document size should produce a non-zero rectangle")
    }

    fn clip_rect_ops(rect: NonZeroRect) -> String {
        format!(
            "{:.6} {:.6} {:.6} {:.6} re W n",
            rect.x(),
            rect.y(),
            rect.width(),
            rect.height()
        )
    }

    fn pdf_bbox(rect: NonZeroRect) -> String {
        format!(
            "{:.6} {:.6} {:.6} {:.6}",
            rect.x(),
            rect.y(),
            rect.x() + rect.width(),
            rect.y() + rect.height()
        )
    }
}
