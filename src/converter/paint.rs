//! Path painting, gradients, masks, and pattern emission.
//!
//! These helpers translate `usvg` paint servers into PDF painting operators and
//! the auxiliary resources needed to reproduce SVG semantics.

use tiny_skia::{Path as TinyPath, Stroke as TinyStroke, StrokeDash};
use usvg::tiny_skia_path;
use usvg::{PaintOrder, SpreadMethod, Stop};

use super::{PdfContext, PdfConverter};

#[derive(Clone, Copy)]
struct GradientStopData {
    offset: f32,
    r: f32,
    g: f32,
    b: f32,
    opacity: f32,
}

impl PdfConverter {
    pub(crate) fn apply_transform(&mut self, transform: &usvg::Transform) {
        self.pdf_ops.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} cm ",
            transform.sx, transform.ky, transform.kx, transform.sy, transform.tx, transform.ty
        ));
    }

    pub(crate) fn apply_graphics_state(&mut self, opacity: f32, blend_mode: usvg::BlendMode) {
        let mut entries = Vec::new();

        if opacity < 1.0 {
            entries.push(format!("/ca {:.6}", opacity));
            entries.push(format!("/CA {:.6}", opacity));
        }

        if !matches!(blend_mode, usvg::BlendMode::Normal) {
            entries.push(format!("/BM/{}", Self::blend_mode_name(blend_mode)));
        }

        if let Some(gs_name) = self.ensure_ext_gstate(&entries) {
            self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
        }
    }

    pub(crate) fn apply_paint(&mut self, paint: &usvg::Paint, opacity: f32, is_fill: bool) {
        match paint {
            usvg::Paint::Color(color) => {
                let r = color.red as f32 / 255.0;
                let g = color.green as f32 / 255.0;
                let b = color.blue as f32 / 255.0;
                if is_fill {
                    self.pdf_ops
                        .push_str(&format!("{:.6} {:.6} {:.6} rg ", r, g, b));
                } else {
                    self.pdf_ops
                        .push_str(&format!("{:.6} {:.6} {:.6} RG ", r, g, b));
                }
            }
            usvg::Paint::LinearGradient(_)
            | usvg::Paint::RadialGradient(_)
            | usvg::Paint::Pattern(_) => {}
        }

        if opacity < 1.0 {
            let entry = if is_fill {
                format!("/ca {:.6}", opacity)
            } else {
                format!("/CA {:.6}", opacity)
            };
            if let Some(gs_name) = self.ensure_ext_gstate(&[entry]) {
                self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
            }
        }
    }

    pub(crate) fn render_path_components(&mut self, path: &usvg::Path) -> Result<(), String> {
        // SVG paint order applies fill and stroke as separate paint operations,
        // so we preserve the declared order instead of collapsing them.
        match path.paint_order() {
            PaintOrder::FillAndStroke => {
                if let Some(fill) = path.fill() {
                    self.render_fill(path, fill)?;
                }
                if let Some(stroke) = path.stroke() {
                    self.render_stroke(path, stroke)?;
                }
            }
            PaintOrder::StrokeAndFill => {
                if let Some(stroke) = path.stroke() {
                    self.render_stroke(path, stroke)?;
                }
                if let Some(fill) = path.fill() {
                    self.render_fill(path, fill)?;
                }
            }
        }

        Ok(())
    }

    fn render_fill(&mut self, path: &usvg::Path, fill: &usvg::Fill) -> Result<(), String> {
        let fill_opacity = fill.opacity().get();
        if fill_opacity <= 0.0 {
            return Ok(());
        }

        match fill.paint() {
            usvg::Paint::Color(_) => {
                self.apply_paint(fill.paint(), fill_opacity, true);
                self.convert_path_data(path);
                match fill.rule() {
                    usvg::FillRule::EvenOdd => self.pdf_ops.push_str("f* "),
                    _ => self.pdf_ops.push_str("f "),
                }
                Ok(())
            }
            usvg::Paint::LinearGradient(gradient) => self.render_linear_gradient_fill(
                path.data(),
                Some(fill.rule()),
                gradient,
                fill_opacity,
            ),
            usvg::Paint::RadialGradient(gradient) => self.render_radial_gradient_fill(
                path.data(),
                Some(fill.rule()),
                gradient,
                fill_opacity,
            ),
            usvg::Paint::Pattern(pattern) => {
                let pattern_name = self.ensure_tiling_pattern(pattern)?;
                self.pdf_ops.push_str("/Pattern cs ");
                self.pdf_ops.push_str(&format!("/{} scn ", pattern_name));
                if fill_opacity < 1.0 {
                    self.apply_fill_opacity(fill_opacity);
                }
                self.convert_path_data(path);
                match fill.rule() {
                    usvg::FillRule::EvenOdd => self.pdf_ops.push_str("f* "),
                    _ => self.pdf_ops.push_str("f "),
                }
                Ok(())
            }
        }
    }

    fn render_stroke(&mut self, path: &usvg::Path, stroke: &usvg::Stroke) -> Result<(), String> {
        let stroke_opacity = stroke.opacity().get();
        if stroke_opacity <= 0.0 {
            return Ok(());
        }

        match stroke.paint() {
            usvg::Paint::Color(_) => {
                self.apply_paint(stroke.paint(), stroke_opacity, false);
                self.apply_stroke_properties(stroke);
                self.convert_path_data(path);
                self.pdf_ops.push_str("S ");
                Ok(())
            }
            usvg::Paint::LinearGradient(gradient) => {
                let outline = self.stroke_outline_path(path.data(), stroke)?;
                self.render_linear_gradient_fill(&outline, None, gradient, stroke_opacity)
            }
            usvg::Paint::RadialGradient(gradient) => {
                let outline = self.stroke_outline_path(path.data(), stroke)?;
                self.render_radial_gradient_fill(&outline, None, gradient, stroke_opacity)
            }
            usvg::Paint::Pattern(pattern) => {
                let pattern_name = self.ensure_tiling_pattern(pattern)?;
                self.pdf_ops.push_str("/Pattern CS ");
                self.pdf_ops.push_str(&format!("/{} SCN ", pattern_name));
                if stroke_opacity < 1.0 {
                    if let Some(gs_name) =
                        self.ensure_ext_gstate(&[format!("/CA {:.6}", stroke_opacity)])
                    {
                        self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
                    }
                }
                self.apply_stroke_properties(stroke);
                self.convert_path_data(path);
                self.pdf_ops.push_str("S ");
                Ok(())
            }
        }
    }

    fn ensure_tiling_pattern(&mut self, pattern: &usvg::Pattern) -> Result<String, String> {
        let key = format!(
            "pattern/{}/{:.6}/{:.6}/{:.6}/{:.6}/{}",
            pattern.id(),
            pattern.rect().x(),
            pattern.rect().y(),
            pattern.rect().width(),
            pattern.rect().height(),
            Self::pdf_matrix(
                pattern
                    .transform()
                    .pre_translate(pattern.rect().x(), pattern.rect().y())
            )
        );
        if let Some(resource) = self.resources.patterns.get(&key) {
            return Ok(resource.name.clone());
        }

        let stream = self.render_pattern_stream(pattern.root())?;
        let rect = pattern.rect();
        // PDF pattern matrices are anchored at the pattern cell origin, while
        // SVG stores the transform separately from the cell rect.
        let matrix = pattern.transform().pre_translate(rect.x(), rect.y());
        let pdf_resources = self.inline_pdf_resource_dict(true);
        let dvi_resources = self.inline_dvi_resource_dict(true);
        let pdf_dict = format!(
            "<</Type/Pattern/PatternType 1/PaintType 1/TilingType 1/BBox [0 0 {:.6} {:.6}] /XStep {:.6} /YStep {:.6} /Matrix [{}] /Resources {} /Filter [/ASCIIHexDecode]>>",
            rect.width(),
            rect.height(),
            rect.width(),
            rect.height(),
            Self::pdf_matrix(matrix),
            if pdf_resources.is_empty() { "<<>>".to_string() } else { pdf_resources }
        );
        let dvi_dict = format!(
            "<</Type/Pattern/PatternType 1/PaintType 1/TilingType 1/BBox [0 0 {:.6} {:.6}] /XStep {:.6} /YStep {:.6} /Matrix [{}] /Resources {} /Filter /ASCIIHexDecode>>",
            rect.width(),
            rect.height(),
            rect.width(),
            rect.height(),
            Self::pdf_matrix(matrix),
            if dvi_resources.is_empty() { "<<>>".to_string() } else { dvi_resources }
        );

        Ok(self.ensure_pattern(key, pdf_dict, dvi_dict, stream))
    }

    pub(crate) fn ensure_mask_ext_gstate(&mut self, mask: &usvg::Mask) -> Result<String, String> {
        let form_name = self.ensure_mask_form(mask)?;
        let subtype = match mask.kind() {
            usvg::MaskType::Alpha => "Alpha",
            usvg::MaskType::Luminance => "Luminosity",
        };
        Ok(self.ensure_soft_mask_ext_gstate(&form_name, subtype))
    }

    fn ensure_mask_form(&mut self, mask: &usvg::Mask) -> Result<String, String> {
        let key = format!(
            "mask/{}/{:?}/{:.6}/{:.6}/{:.6}/{:.6}",
            mask.id(),
            mask.kind(),
            mask.rect().x(),
            mask.rect().y(),
            mask.rect().width(),
            mask.rect().height()
        );
        if let Some(resource) = self.resources.forms.get(&key) {
            return Ok(resource.name.clone());
        }

        let stream = self.render_mask_stream(mask)?;
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

    fn render_mask_stream(&mut self, mask: &usvg::Mask) -> Result<Vec<u8>, String> {
        let saved_ops = std::mem::take(&mut self.pdf_ops);
        let saved_ctx = std::mem::replace(&mut self.ctx, PdfContext::new());

        let result = (|| {
            self.pdf_ops
                .push_str(&format!("q 1 0 0 -1 0 {:.6} cm ", self.size.height()));

            if let Some(parent_mask) = mask.mask() {
                let gs_name = self.ensure_mask_ext_gstate(parent_mask)?;
                self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
            }

            self.append_rect_path(
                mask.rect().x(),
                mask.rect().y(),
                mask.rect().width(),
                mask.rect().height(),
            );
            self.pdf_ops.push_str("W n ");

            self.process_group(None, mask.root(), &usvg::Transform::identity())?;
            self.pdf_ops.push_str("Q");

            Ok::<(), String>(())
        })();

        let stream = self.pdf_ops.clone().into_bytes();
        self.pdf_ops = saved_ops;
        self.ctx = saved_ctx;

        result.map(|_| stream)
    }

    fn render_pattern_stream(&mut self, root: &usvg::Group) -> Result<Vec<u8>, String> {
        let saved_ops = std::mem::take(&mut self.pdf_ops);
        let saved_ctx = std::mem::replace(&mut self.ctx, PdfContext::new());

        let result = self.process_group(None, root, &usvg::Transform::identity());
        let stream = self.pdf_ops.clone().into_bytes();

        self.pdf_ops = saved_ops;
        self.ctx = saved_ctx;

        result.map(|_| stream)
    }

    fn render_linear_gradient_fill(
        &mut self,
        clip_path: &TinyPath,
        fill_rule: Option<usvg::FillRule>,
        gradient: &usvg::LinearGradient,
        paint_opacity: f32,
    ) -> Result<(), String> {
        if !matches!(gradient.spread_method(), SpreadMethod::Pad) {
            return self.render_linear_spread_gradient_fill(
                clip_path,
                fill_rule,
                gradient,
                paint_opacity,
            );
        }

        let shading_name = self.ensure_linear_shading(gradient);
        let (soft_mask_gs, effective_opacity) =
            self.gradient_soft_mask_state_for_linear(gradient, paint_opacity);
        self.paint_shading_clip(
            clip_path,
            fill_rule,
            &shading_name,
            effective_opacity,
            soft_mask_gs.as_deref(),
        );
        Ok(())
    }

    fn render_radial_gradient_fill(
        &mut self,
        clip_path: &TinyPath,
        fill_rule: Option<usvg::FillRule>,
        gradient: &usvg::RadialGradient,
        paint_opacity: f32,
    ) -> Result<(), String> {
        if !matches!(gradient.spread_method(), SpreadMethod::Pad) {
            return self.render_radial_spread_gradient_fill(
                clip_path,
                fill_rule,
                gradient,
                paint_opacity,
            );
        }

        let shading_name = self.ensure_radial_shading(gradient);
        let (soft_mask_gs, effective_opacity) =
            self.gradient_soft_mask_state_for_radial(gradient, paint_opacity);
        self.paint_shading_clip(
            clip_path,
            fill_rule,
            &shading_name,
            effective_opacity,
            soft_mask_gs.as_deref(),
        );
        Ok(())
    }

    fn render_linear_spread_gradient_fill(
        &mut self,
        clip_path: &TinyPath,
        fill_rule: Option<usvg::FillRule>,
        gradient: &usvg::LinearGradient,
        paint_opacity: f32,
    ) -> Result<(), String> {
        let (soft_mask_gs, effective_opacity) = match Self::uniform_stop_opacity(gradient.stops()) {
            Some(stop_opacity) => (None, paint_opacity * stop_opacity),
            None => {
                let form_name =
                    self.ensure_linear_spread_alpha_form(clip_path, fill_rule, gradient)?;
                let gs_name = self.ensure_soft_mask_ext_gstate(&form_name, "Alpha");
                (Some(gs_name), paint_opacity)
            }
        };

        self.paint_linear_spread_segments(
            clip_path,
            fill_rule,
            gradient,
            effective_opacity,
            soft_mask_gs.as_deref(),
            false,
        )
    }

    fn render_radial_spread_gradient_fill(
        &mut self,
        clip_path: &TinyPath,
        fill_rule: Option<usvg::FillRule>,
        gradient: &usvg::RadialGradient,
        paint_opacity: f32,
    ) -> Result<(), String> {
        let (soft_mask_gs, effective_opacity) = match Self::uniform_stop_opacity(gradient.stops()) {
            Some(stop_opacity) => (None, paint_opacity * stop_opacity),
            None => {
                let form_name =
                    self.ensure_radial_spread_alpha_form(clip_path, fill_rule, gradient)?;
                let gs_name = self.ensure_soft_mask_ext_gstate(&form_name, "Alpha");
                (Some(gs_name), paint_opacity)
            }
        };

        self.paint_radial_spread_segments(
            clip_path,
            fill_rule,
            gradient,
            effective_opacity,
            soft_mask_gs.as_deref(),
            false,
        )
    }

    fn paint_shading_clip(
        &mut self,
        clip_path: &TinyPath,
        fill_rule: Option<usvg::FillRule>,
        shading_name: &str,
        opacity: f32,
        soft_mask_gs_name: Option<&str>,
    ) {
        if opacity <= 0.0 {
            return;
        }

        self.pdf_ops.push_str("q ");
        // Gradient opacity is represented either as a uniform fill alpha or as
        // a soft mask, but the geometric clip path is shared between both.
        if let Some(gs_name) = soft_mask_gs_name {
            self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
        }
        self.apply_fill_opacity(opacity);
        self.append_tiny_skia_path(clip_path);
        match fill_rule {
            Some(usvg::FillRule::EvenOdd) => self.pdf_ops.push_str("W* n "),
            _ => self.pdf_ops.push_str("W n "),
        }
        self.pdf_ops.push_str(&format!("/{} sh ", shading_name));
        self.pdf_ops.push_str("Q ");
    }

    fn ensure_linear_shading(&mut self, gradient: &usvg::LinearGradient) -> String {
        let key = format!(
            "axial/{}/{:.6}/{:.6}/{:.6}/{:.6}/{}",
            Self::pdf_matrix(gradient.transform()),
            gradient.x1(),
            gradient.y1(),
            gradient.x2(),
            gradient.y2(),
            Self::gradient_stops_key(gradient.stops())
        );
        let dict = format!(
            "<</ShadingType 2 /ColorSpace /DeviceRGB /Coords [{:.6} {:.6} {:.6} {:.6}] /Function {} /Extend [true true] /Matrix [{}] /AntiAlias true>>",
            gradient.x1(),
            gradient.y1(),
            gradient.x2(),
            gradient.y2(),
            Self::pdf_function_from_stops(gradient.stops()),
            Self::pdf_matrix(gradient.transform())
        );
        self.ensure_shading(key, dict)
    }

    fn ensure_radial_shading(&mut self, gradient: &usvg::RadialGradient) -> String {
        let key = format!(
            "radial/{}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}/{}",
            Self::pdf_matrix(gradient.transform()),
            gradient.fx(),
            gradient.fy(),
            gradient.cx(),
            gradient.cy(),
            gradient.r().get(),
            Self::gradient_stops_key(gradient.stops())
        );
        let dict = format!(
            "<</ShadingType 3 /ColorSpace /DeviceRGB /Coords [{:.6} {:.6} 0 {:.6} {:.6} {:.6}] /Function {} /Extend [true true] /Matrix [{}] /AntiAlias true>>",
            gradient.fx(),
            gradient.fy(),
            gradient.cx(),
            gradient.cy(),
            gradient.r().get(),
            Self::pdf_function_from_stops(gradient.stops()),
            Self::pdf_matrix(gradient.transform())
        );
        self.ensure_shading(key, dict)
    }

    fn ensure_linear_alpha_shading(&mut self, gradient: &usvg::LinearGradient) -> String {
        let key = format!(
            "alpha-axial/{}/{:.6}/{:.6}/{:.6}/{:.6}/{}",
            Self::pdf_matrix(gradient.transform()),
            gradient.x1(),
            gradient.y1(),
            gradient.x2(),
            gradient.y2(),
            Self::gradient_stops_key(gradient.stops())
        );
        let dict = format!(
            "<</ShadingType 2 /ColorSpace /DeviceGray /Coords [{:.6} {:.6} {:.6} {:.6}] /Function {} /Extend [true true] /Matrix [{}] /AntiAlias true>>",
            gradient.x1(),
            gradient.y1(),
            gradient.x2(),
            gradient.y2(),
            Self::pdf_function_from_opacity_stops(gradient.stops()),
            Self::pdf_matrix(gradient.transform())
        );
        self.ensure_shading(key, dict)
    }

    fn ensure_radial_alpha_shading(&mut self, gradient: &usvg::RadialGradient) -> String {
        let key = format!(
            "alpha-radial/{}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}/{}",
            Self::pdf_matrix(gradient.transform()),
            gradient.fx(),
            gradient.fy(),
            gradient.cx(),
            gradient.cy(),
            gradient.r().get(),
            Self::gradient_stops_key(gradient.stops())
        );
        let dict = format!(
            "<</ShadingType 3 /ColorSpace /DeviceGray /Coords [{:.6} {:.6} 0 {:.6} {:.6} {:.6}] /Function {} /Extend [true true] /Matrix [{}] /AntiAlias true>>",
            gradient.fx(),
            gradient.fy(),
            gradient.cx(),
            gradient.cy(),
            gradient.r().get(),
            Self::pdf_function_from_opacity_stops(gradient.stops()),
            Self::pdf_matrix(gradient.transform())
        );
        self.ensure_shading(key, dict)
    }

    fn ensure_linear_segment_shading(
        &mut self,
        gradient: &usvg::LinearGradient,
        segment: i32,
        alpha_only: bool,
    ) -> String {
        let reversed = Self::spread_segment_reversed(gradient.spread_method(), segment);
        let t0 = segment as f32;
        let t1 = t0 + 1.0;
        let dx = gradient.x2() - gradient.x1();
        let dy = gradient.y2() - gradient.y1();
        let x0 = gradient.x1() + dx * t0;
        let y0 = gradient.y1() + dy * t0;
        let x1 = gradient.x1() + dx * t1;
        let y1 = gradient.y1() + dy * t1;
        let stops_key = Self::gradient_stops_key_direction(gradient.stops(), reversed);
        let key_prefix = if alpha_only {
            "alpha-axial-segment"
        } else {
            "axial-segment"
        };
        let key = format!(
            "{}/{}/{:.6}/{:.6}/{:.6}/{:.6}/{}",
            key_prefix,
            Self::pdf_matrix(gradient.transform()),
            x0,
            y0,
            x1,
            y1,
            stops_key
        );
        let function = if alpha_only {
            Self::pdf_function_from_opacity_stops_direction(gradient.stops(), reversed)
        } else {
            Self::pdf_function_from_stops_direction(gradient.stops(), reversed)
        };
        let color_space = if alpha_only {
            "DeviceGray"
        } else {
            "DeviceRGB"
        };
        let dict = format!(
            "<</ShadingType 2 /ColorSpace /{} /Coords [{:.6} {:.6} {:.6} {:.6}] /Function {} /Extend [false false] /Matrix [{}] /AntiAlias true>>",
            color_space,
            x0,
            y0,
            x1,
            y1,
            function,
            Self::pdf_matrix(gradient.transform())
        );
        self.ensure_shading(key, dict)
    }

    fn ensure_radial_segment_shading(
        &mut self,
        gradient: &usvg::RadialGradient,
        segment: i32,
        alpha_only: bool,
    ) -> String {
        let reversed = Self::spread_segment_reversed(gradient.spread_method(), segment);
        let t0 = segment.max(0) as f32;
        let t1 = t0 + 1.0;
        let dcx = gradient.cx() - gradient.fx();
        let dcy = gradient.cy() - gradient.fy();
        let x0 = gradient.fx() + dcx * t0;
        let y0 = gradient.fy() + dcy * t0;
        let x1 = gradient.fx() + dcx * t1;
        let y1 = gradient.fy() + dcy * t1;
        let r0 = gradient.r().get() * t0;
        let r1 = gradient.r().get() * t1;
        let stops_key = Self::gradient_stops_key_direction(gradient.stops(), reversed);
        let key_prefix = if alpha_only {
            "alpha-radial-segment"
        } else {
            "radial-segment"
        };
        let key = format!(
            "{}/{}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}/{:.6}/{}",
            key_prefix,
            Self::pdf_matrix(gradient.transform()),
            x0,
            y0,
            r0,
            x1,
            y1,
            r1,
            stops_key
        );
        let function = if alpha_only {
            Self::pdf_function_from_opacity_stops_direction(gradient.stops(), reversed)
        } else {
            Self::pdf_function_from_stops_direction(gradient.stops(), reversed)
        };
        let color_space = if alpha_only {
            "DeviceGray"
        } else {
            "DeviceRGB"
        };
        let dict = format!(
            "<</ShadingType 3 /ColorSpace /{} /Coords [{:.6} {:.6} {:.6} {:.6} {:.6} {:.6}] /Function {} /Extend [false false] /Matrix [{}] /AntiAlias true>>",
            color_space,
            x0,
            y0,
            r0,
            x1,
            y1,
            r1,
            function,
            Self::pdf_matrix(gradient.transform())
        );
        self.ensure_shading(key, dict)
    }

    fn gradient_soft_mask_state_for_linear(
        &mut self,
        gradient: &usvg::LinearGradient,
        paint_opacity: f32,
    ) -> (Option<String>, f32) {
        match Self::uniform_stop_opacity(gradient.stops()) {
            Some(stop_opacity) => (None, paint_opacity * stop_opacity),
            None => {
                let shading_name = self.ensure_linear_alpha_shading(gradient);
                let form_name = self.ensure_gradient_alpha_form(&shading_name);
                let gs_name = self.ensure_soft_mask_ext_gstate(&form_name, "Alpha");
                (Some(gs_name), paint_opacity)
            }
        }
    }

    fn gradient_soft_mask_state_for_radial(
        &mut self,
        gradient: &usvg::RadialGradient,
        paint_opacity: f32,
    ) -> (Option<String>, f32) {
        match Self::uniform_stop_opacity(gradient.stops()) {
            Some(stop_opacity) => (None, paint_opacity * stop_opacity),
            None => {
                let shading_name = self.ensure_radial_alpha_shading(gradient);
                let form_name = self.ensure_gradient_alpha_form(&shading_name);
                let gs_name = self.ensure_soft_mask_ext_gstate(&form_name, "Alpha");
                (Some(gs_name), paint_opacity)
            }
        }
    }

    fn ensure_linear_spread_alpha_form(
        &mut self,
        clip_path: &TinyPath,
        fill_rule: Option<usvg::FillRule>,
        gradient: &usvg::LinearGradient,
    ) -> Result<String, String> {
        let key = format!(
            "alpha-spread-linear/{}/{:.6}/{:.6}/{:.6}/{:.6}/{}",
            self.resources.get_next_id(),
            clip_path.bounds().left(),
            clip_path.bounds().top(),
            clip_path.bounds().right(),
            clip_path.bounds().bottom(),
            Self::gradient_stops_key_direction(gradient.stops(), false)
        );
        let stream = self.capture_stream(|converter| {
            converter
                .pdf_ops
                .push_str(&format!("q 1 0 0 -1 0 {:.6} cm ", converter.size.height()));
            converter
                .paint_linear_spread_segments(clip_path, fill_rule, gradient, 1.0, None, true)?;
            converter.pdf_ops.push_str("Q");
            Ok(())
        })?;
        Ok(self.ensure_spread_alpha_form(key, stream))
    }

    fn ensure_radial_spread_alpha_form(
        &mut self,
        clip_path: &TinyPath,
        fill_rule: Option<usvg::FillRule>,
        gradient: &usvg::RadialGradient,
    ) -> Result<String, String> {
        let key = format!(
            "alpha-spread-radial/{}/{:.6}/{:.6}/{:.6}/{:.6}/{}",
            self.resources.get_next_id(),
            clip_path.bounds().left(),
            clip_path.bounds().top(),
            clip_path.bounds().right(),
            clip_path.bounds().bottom(),
            Self::gradient_stops_key_direction(gradient.stops(), false)
        );
        let stream = self.capture_stream(|converter| {
            converter
                .pdf_ops
                .push_str(&format!("q 1 0 0 -1 0 {:.6} cm ", converter.size.height()));
            converter
                .paint_radial_spread_segments(clip_path, fill_rule, gradient, 1.0, None, true)?;
            converter.pdf_ops.push_str("Q");
            Ok(())
        })?;
        Ok(self.ensure_spread_alpha_form(key, stream))
    }

    fn ensure_spread_alpha_form(&mut self, key: String, stream: Vec<u8>) -> String {
        let pdf_resources = self.inline_pdf_resource_dict(true);
        let dvi_resources = self.inline_dvi_resource_dict(true);
        let pdf_dict = format!(
            "<</Type/XObject/Subtype/Form/BBox [0 0 {:.6} {:.6}] /Group <</S /Transparency /CS /DeviceGray>> /Resources {} /Filter [/ASCIIHexDecode]>>",
            self.size.width(),
            self.size.height(),
            if pdf_resources.is_empty() { "<<>>".to_string() } else { pdf_resources }
        );
        let dvi_dict = format!(
            "<</Type/XObject/Subtype/Form/BBox [0 0 {:.6} {:.6}] /Group <</S /Transparency /CS /DeviceGray>> /Resources {} /Filter /ASCIIHexDecode>>",
            self.size.width(),
            self.size.height(),
            if dvi_resources.is_empty() { "<<>>".to_string() } else { dvi_resources }
        );

        self.ensure_form(key, pdf_dict, dvi_dict, stream)
    }

    fn paint_linear_spread_segments(
        &mut self,
        clip_path: &TinyPath,
        fill_rule: Option<usvg::FillRule>,
        gradient: &usvg::LinearGradient,
        opacity: f32,
        soft_mask_gs_name: Option<&str>,
        alpha_only: bool,
    ) -> Result<(), String> {
        if opacity <= 0.0 {
            return Ok(());
        }

        let inv = gradient
            .transform()
            .invert()
            .ok_or_else(|| "Non-invertible linear gradient transform".to_string())?;
        let bounds = clip_path.bounds();
        let corners = Self::clip_bounds_points(bounds);
        let d = (gradient.x2() - gradient.x1(), gradient.y2() - gradient.y1());
        let len2 = d.0 * d.0 + d.1 * d.1;
        if len2 <= 1e-6 {
            let shading_name = if alpha_only {
                self.ensure_linear_alpha_shading(gradient)
            } else {
                self.ensure_linear_shading(gradient)
            };
            self.paint_shading_clip(
                clip_path,
                fill_rule,
                &shading_name,
                opacity,
                soft_mask_gs_name,
            );
            return Ok(());
        }

        let grad_corners = corners
            .iter()
            .map(|&(x, y)| Self::transform_point(inv, x, y))
            .collect::<Vec<_>>();
        let p1 = (gradient.x1(), gradient.y1());
        let min_t = grad_corners
            .iter()
            .map(|&(x, y)| ((x - p1.0) * d.0 + (y - p1.1) * d.1) / len2)
            .fold(f32::INFINITY, f32::min);
        let max_t = grad_corners
            .iter()
            .map(|&(x, y)| ((x - p1.0) * d.0 + (y - p1.1) * d.1) / len2)
            .fold(f32::NEG_INFINITY, f32::max);
        let start = min_t.floor() as i32;
        let end = max_t.ceil() as i32;
        let len = len2.sqrt();
        let n = (-d.1 / len, d.0 / len);
        let extent = grad_corners
            .iter()
            .map(|&(x, y)| ((x - p1.0) * n.0 + (y - p1.1) * n.1).abs())
            .fold(len, f32::max)
            + len * 4.0
            + self.size.width().max(self.size.height());

        self.pdf_ops.push_str("q ");
        if let Some(gs_name) = soft_mask_gs_name {
            self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
        }
        self.apply_fill_opacity(opacity);
        self.append_tiny_skia_path(clip_path);
        match fill_rule {
            Some(usvg::FillRule::EvenOdd) => self.pdf_ops.push_str("W* n "),
            _ => self.pdf_ops.push_str("W n "),
        }

        for segment in start..end {
            let t0 = segment as f32;
            let t1 = t0 + 1.0;
            let start_point = (p1.0 + d.0 * t0, p1.1 + d.1 * t0);
            let end_point = (p1.0 + d.0 * t1, p1.1 + d.1 * t1);
            let polygon = [
                Self::transform_point(
                    gradient.transform(),
                    start_point.0 + n.0 * extent,
                    start_point.1 + n.1 * extent,
                ),
                Self::transform_point(
                    gradient.transform(),
                    end_point.0 + n.0 * extent,
                    end_point.1 + n.1 * extent,
                ),
                Self::transform_point(
                    gradient.transform(),
                    end_point.0 - n.0 * extent,
                    end_point.1 - n.1 * extent,
                ),
                Self::transform_point(
                    gradient.transform(),
                    start_point.0 - n.0 * extent,
                    start_point.1 - n.1 * extent,
                ),
            ];
            let shading_name = self.ensure_linear_segment_shading(gradient, segment, alpha_only);

            self.pdf_ops.push_str("q ");
            self.append_polygon_path(&polygon);
            self.pdf_ops.push_str("W n ");
            self.pdf_ops.push_str(&format!("/{} sh ", shading_name));
            self.pdf_ops.push_str("Q ");
        }

        self.pdf_ops.push_str("Q ");
        Ok(())
    }

    fn paint_radial_spread_segments(
        &mut self,
        clip_path: &TinyPath,
        fill_rule: Option<usvg::FillRule>,
        gradient: &usvg::RadialGradient,
        opacity: f32,
        soft_mask_gs_name: Option<&str>,
        alpha_only: bool,
    ) -> Result<(), String> {
        if opacity <= 0.0 {
            return Ok(());
        }

        let inv = gradient
            .transform()
            .invert()
            .ok_or_else(|| "Non-invertible radial gradient transform".to_string())?;
        let bounds = clip_path.bounds();
        let samples = Self::clip_bounds_points(bounds)
            .into_iter()
            .chain(std::iter::once((
                (bounds.left() + bounds.right()) * 0.5,
                (bounds.top() + bounds.bottom()) * 0.5,
            )))
            .collect::<Vec<_>>();
        let max_t = samples
            .iter()
            .filter_map(|&(x, y)| {
                let point = Self::transform_point(inv, x, y);
                Self::solve_radial_t(
                    point,
                    (gradient.fx(), gradient.fy()),
                    (gradient.cx(), gradient.cy()),
                    gradient.r().get(),
                )
            })
            .fold(1.0, f32::max);
        let end = max_t.ceil().max(1.0) as i32;

        self.pdf_ops.push_str("q ");
        if let Some(gs_name) = soft_mask_gs_name {
            self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
        }
        self.apply_fill_opacity(opacity);
        self.append_tiny_skia_path(clip_path);
        match fill_rule {
            Some(usvg::FillRule::EvenOdd) => self.pdf_ops.push_str("W* n "),
            _ => self.pdf_ops.push_str("W n "),
        }

        for segment in 0..end {
            let shading_name = self.ensure_radial_segment_shading(gradient, segment, alpha_only);
            self.pdf_ops.push_str("q ");
            self.append_radial_segment_clip(gradient, segment as f32, segment as f32 + 1.0);
            if segment > 0 {
                self.pdf_ops.push_str("W* n ");
            } else {
                self.pdf_ops.push_str("W n ");
            }
            self.pdf_ops.push_str(&format!("/{} sh ", shading_name));
            self.pdf_ops.push_str("Q ");
        }

        self.pdf_ops.push_str("Q ");
        Ok(())
    }

    fn ensure_gradient_alpha_form(&mut self, shading_name: &str) -> String {
        let key = format!("alpha-form/{shading_name}");
        let pdf_dict = format!(
            "<</Type/XObject/Subtype/Form/BBox [0 0 {:.6} {:.6}] /Group <</S /Transparency /CS /DeviceGray>> /Resources <</Shading<</{} {}>>>> /Filter [/ASCIIHexDecode]>>",
            self.size.width(),
            self.size.height(),
            shading_name,
            Self::tex_obj_ref(shading_name)
        );
        let dvi_dict = format!(
            "<</Type/XObject/Subtype/Form/BBox [0 0 {:.6} {:.6}] /Group <</S /Transparency /CS /DeviceGray>> /Resources <</Shading<</{} @{}>>>> /Filter /ASCIIHexDecode>>",
            self.size.width(),
            self.size.height(),
            shading_name,
            shading_name
        );
        let stream = format!(
            "q 1 0 0 -1 0 {:.6} cm /{} sh Q",
            self.size.height(),
            shading_name
        )
        .into_bytes();

        self.ensure_form(key, pdf_dict, dvi_dict, stream)
    }

    fn stroke_outline_path(
        &self,
        path: &TinyPath,
        stroke: &usvg::Stroke,
    ) -> Result<TinyPath, String> {
        let mut outline_stroke = TinyStroke {
            width: stroke.width().get(),
            miter_limit: stroke.miterlimit().get(),
            line_cap: match stroke.linecap() {
                usvg::LineCap::Butt => tiny_skia::LineCap::Butt,
                usvg::LineCap::Round => tiny_skia::LineCap::Round,
                usvg::LineCap::Square => tiny_skia::LineCap::Square,
            },
            line_join: match stroke.linejoin() {
                usvg::LineJoin::Miter => tiny_skia::LineJoin::Miter,
                usvg::LineJoin::MiterClip => tiny_skia::LineJoin::MiterClip,
                usvg::LineJoin::Round => tiny_skia::LineJoin::Round,
                usvg::LineJoin::Bevel => tiny_skia::LineJoin::Bevel,
            },
            dash: None,
        };

        if let Some(dasharray) = stroke.dasharray() {
            outline_stroke.dash = StrokeDash::new(dasharray.to_vec(), stroke.dashoffset());
        }

        path.stroke(&outline_stroke, 1.0)
            .ok_or_else(|| "Failed to convert stroked path into an outline".to_string())
    }

    fn append_polygon_path(&mut self, points: &[(f32, f32)]) {
        if let Some(&(x0, y0)) = points.first() {
            self.pdf_ops.push_str(&format!("{:.6} {:.6} m ", x0, y0));
            for &(x, y) in &points[1..] {
                self.pdf_ops.push_str(&format!("{:.6} {:.6} l ", x, y));
            }
            self.pdf_ops.push_str("h ");
            self.ctx.current_point = Some((x0, y0));
            self.ctx.subpath_start = Some((x0, y0));
        }
    }

    fn append_radial_segment_clip(&mut self, gradient: &usvg::RadialGradient, t0: f32, t1: f32) {
        let dcx = gradient.cx() - gradient.fx();
        let dcy = gradient.cy() - gradient.fy();
        let outer_center = (gradient.fx() + dcx * t1, gradient.fy() + dcy * t1);
        let inner_center = (gradient.fx() + dcx * t0, gradient.fy() + dcy * t0);
        let outer_radius = gradient.r().get() * t1;
        let inner_radius = gradient.r().get() * t0;

        self.append_transformed_circle_path(
            outer_center.0,
            outer_center.1,
            outer_radius,
            gradient.transform(),
        );
        if t0 > 0.0 {
            self.append_transformed_circle_path(
                inner_center.0,
                inner_center.1,
                inner_radius,
                gradient.transform(),
            );
        }
    }

    fn append_transformed_circle_path(
        &mut self,
        cx: f32,
        cy: f32,
        r: f32,
        transform: usvg::Transform,
    ) {
        if r <= 0.0 {
            return;
        }

        const KAPPA: f32 = 0.552_284_8;
        let points = [
            (cx + r, cy),
            (cx + r, cy + KAPPA * r),
            (cx + KAPPA * r, cy + r),
            (cx, cy + r),
            (cx - KAPPA * r, cy + r),
            (cx - r, cy + KAPPA * r),
            (cx - r, cy),
            (cx - r, cy - KAPPA * r),
            (cx - KAPPA * r, cy - r),
            (cx, cy - r),
            (cx + KAPPA * r, cy - r),
            (cx + r, cy - KAPPA * r),
            (cx + r, cy),
        ]
        .map(|(x, y)| Self::transform_point(transform, x, y));

        self.pdf_ops
            .push_str(&format!("{:.6} {:.6} m ", points[0].0, points[0].1));
        self.pdf_ops.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c ",
            points[1].0, points[1].1, points[2].0, points[2].1, points[3].0, points[3].1
        ));
        self.pdf_ops.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c ",
            points[4].0, points[4].1, points[5].0, points[5].1, points[6].0, points[6].1
        ));
        self.pdf_ops.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c ",
            points[7].0, points[7].1, points[8].0, points[8].1, points[9].0, points[9].1
        ));
        self.pdf_ops.push_str(&format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c h ",
            points[10].0, points[10].1, points[11].0, points[11].1, points[12].0, points[12].1
        ));
        self.ctx.current_point = Some(points[0]);
        self.ctx.subpath_start = Some(points[0]);
    }

    fn clip_bounds_points(bounds: tiny_skia_path::Rect) -> Vec<(f32, f32)> {
        vec![
            (bounds.left(), bounds.top()),
            (bounds.right(), bounds.top()),
            (bounds.right(), bounds.bottom()),
            (bounds.left(), bounds.bottom()),
            ((bounds.left() + bounds.right()) * 0.5, bounds.top()),
            (bounds.right(), (bounds.top() + bounds.bottom()) * 0.5),
            ((bounds.left() + bounds.right()) * 0.5, bounds.bottom()),
            (bounds.left(), (bounds.top() + bounds.bottom()) * 0.5),
        ]
    }

    fn transform_point(transform: usvg::Transform, x: f32, y: f32) -> (f32, f32) {
        (
            transform.sx * x + transform.kx * y + transform.tx,
            transform.ky * x + transform.sy * y + transform.ty,
        )
    }

    fn solve_radial_t(
        point: (f32, f32),
        focus: (f32, f32),
        center: (f32, f32),
        radius: f32,
    ) -> Option<f32> {
        let qx = point.0 - focus.0;
        let qy = point.1 - focus.1;
        let dcx = center.0 - focus.0;
        let dcy = center.1 - focus.1;
        let a = dcx * dcx + dcy * dcy - radius * radius;
        let b = -2.0 * (qx * dcx + qy * dcy);
        let c = qx * qx + qy * qy;

        if a.abs() <= 1e-6 {
            if b.abs() <= 1e-6 {
                return None;
            }
            let t = -c / b;
            return (t >= 0.0).then_some(t);
        }

        let disc = b * b - 4.0 * a * c;
        if disc < 0.0 {
            return None;
        }

        let sqrt_disc = disc.sqrt();
        let roots = [(-b - sqrt_disc) / (2.0 * a), (-b + sqrt_disc) / (2.0 * a)];
        roots
            .into_iter()
            .filter(|root| *root >= 0.0 && root.is_finite())
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    fn apply_fill_opacity(&mut self, opacity: f32) {
        if opacity >= 1.0 {
            return;
        }

        if let Some(gs_name) = self.ensure_ext_gstate(&[format!("/ca {:.6}", opacity)]) {
            self.pdf_ops.push_str(&format!("/{} gs ", gs_name));
        }
    }

    pub(crate) fn append_tiny_skia_path(&mut self, path: &TinyPath) {
        self.ctx.current_point = None;
        self.ctx.subpath_start = None;

        for segment in path.segments() {
            match segment {
                tiny_skia::PathSegment::MoveTo(point) => {
                    self.pdf_ops
                        .push_str(&format!("{:.6} {:.6} m ", point.x, point.y));
                    self.ctx.current_point = Some((point.x, point.y));
                    self.ctx.subpath_start = Some((point.x, point.y));
                }
                tiny_skia::PathSegment::LineTo(point) => {
                    self.pdf_ops
                        .push_str(&format!("{:.6} {:.6} l ", point.x, point.y));
                    self.ctx.current_point = Some((point.x, point.y));
                }
                tiny_skia::PathSegment::CubicTo(p1, p2, p) => {
                    self.pdf_ops.push_str(&format!(
                        "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c ",
                        p1.x, p1.y, p2.x, p2.y, p.x, p.y
                    ));
                    self.ctx.current_point = Some((p.x, p.y));
                }
                tiny_skia::PathSegment::QuadTo(p1, p) => {
                    if let Some((x0, y0)) = self.ctx.current_point {
                        let c1x = x0 + (2.0 / 3.0) * (p1.x - x0);
                        let c1y = y0 + (2.0 / 3.0) * (p1.y - y0);
                        let c2x = p.x + (2.0 / 3.0) * (p1.x - p.x);
                        let c2y = p.y + (2.0 / 3.0) * (p1.y - p.y);
                        self.pdf_ops.push_str(&format!(
                            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c ",
                            c1x, c1y, c2x, c2y, p.x, p.y
                        ));
                    } else {
                        self.pdf_ops.push_str(&format!(
                            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6} c ",
                            p1.x, p1.y, p1.x, p1.y, p.x, p.y
                        ));
                    }
                    self.ctx.current_point = Some((p.x, p.y));
                }
                tiny_skia::PathSegment::Close => {
                    self.pdf_ops.push_str("h ");
                    self.ctx.current_point = self.ctx.subpath_start;
                }
            }
        }
    }

    pub(crate) fn append_rect_path(&mut self, x: f32, y: f32, width: f32, height: f32) {
        self.pdf_ops.push_str(&format!(
            "{:.6} {:.6} m {:.6} {:.6} l {:.6} {:.6} l {:.6} {:.6} l h ",
            x,
            y,
            x + width,
            y,
            x + width,
            y + height,
            x,
            y + height
        ));
        self.ctx.current_point = Some((x, y));
        self.ctx.subpath_start = Some((x, y));
    }

    fn pdf_function_from_stops(stops: &[Stop]) -> String {
        let stops = Self::normalized_gradient_stops_direction(stops, false);

        if stops.len() == 2 {
            return Self::type2_rgb_function(&stops[0], &stops[1]);
        }

        let functions = stops
            .windows(2)
            .map(|pair| Self::type2_rgb_function(&pair[0], &pair[1]))
            .collect::<Vec<_>>()
            .join(" ");
        let bounds = stops[1..stops.len() - 1]
            .iter()
            .map(|stop| format!("{:.6}", stop.offset))
            .collect::<Vec<_>>()
            .join(" ");
        let encode = (0..stops.len() - 1)
            .map(|_| "0 1".to_string())
            .collect::<Vec<_>>()
            .join(" ");

        format!(
            "<< /FunctionType 3 /Domain [0 1] /Functions [{}] /Bounds [{}] /Encode [{}] >>",
            functions, bounds, encode
        )
    }

    fn pdf_function_from_opacity_stops(stops: &[Stop]) -> String {
        let stops = Self::normalized_gradient_stops_direction(stops, false);

        if stops.len() == 2 {
            return Self::type2_gray_function(&stops[0], &stops[1]);
        }

        let functions = stops
            .windows(2)
            .map(|pair| Self::type2_gray_function(&pair[0], &pair[1]))
            .collect::<Vec<_>>()
            .join(" ");
        let bounds = stops[1..stops.len() - 1]
            .iter()
            .map(|stop| format!("{:.6}", stop.offset))
            .collect::<Vec<_>>()
            .join(" ");
        let encode = (0..stops.len() - 1)
            .map(|_| "0 1".to_string())
            .collect::<Vec<_>>()
            .join(" ");

        format!(
            "<< /FunctionType 3 /Domain [0 1] /Functions [{}] /Bounds [{}] /Encode [{}] >>",
            functions, bounds, encode
        )
    }

    fn normalized_gradient_stops(stops: &[Stop]) -> Vec<GradientStopData> {
        let mut out = stops
            .iter()
            .map(|stop| GradientStopData {
                offset: stop.offset().get().clamp(0.0, 1.0),
                r: stop.color().red as f32 / 255.0,
                g: stop.color().green as f32 / 255.0,
                b: stop.color().blue as f32 / 255.0,
                opacity: stop.opacity().get(),
            })
            .collect::<Vec<_>>();

        if out.is_empty() {
            return vec![
                GradientStopData {
                    offset: 0.0,
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    opacity: 1.0,
                },
                GradientStopData {
                    offset: 1.0,
                    r: 0.0,
                    g: 0.0,
                    b: 0.0,
                    opacity: 1.0,
                },
            ];
        }

        if out[0].offset > 0.0 {
            out.insert(
                0,
                GradientStopData {
                    offset: 0.0,
                    ..out[0]
                },
            );
        }

        if out.last().map(|stop| stop.offset).unwrap_or(1.0) < 1.0 {
            let last = *out.last().unwrap();
            out.push(GradientStopData {
                offset: 1.0,
                ..last
            });
        }

        let epsilon = 0.0001;
        let last_index = out.len().saturating_sub(1);
        for i in 1..out.len() {
            if out[i].offset <= out[i - 1].offset {
                let upper_bound = if i == last_index {
                    1.0
                } else {
                    out[i + 1].offset.max(out[i - 1].offset + epsilon)
                };
                out[i].offset = (out[i - 1].offset + epsilon).min(upper_bound).min(1.0);
            }
        }

        if out.len() == 1 {
            out.push(GradientStopData {
                offset: 1.0,
                ..out[0]
            });
        }

        out
    }

    fn normalized_gradient_stops_direction(
        stops: &[Stop],
        reversed: bool,
    ) -> Vec<GradientStopData> {
        let out = Self::normalized_gradient_stops(stops);
        if !reversed {
            return out;
        }

        out.into_iter()
            .rev()
            .map(|mut stop| {
                stop.offset = 1.0 - stop.offset;
                stop
            })
            .collect()
    }

    fn uniform_stop_opacity(stops: &[Stop]) -> Option<f32> {
        let mut iter = stops.iter();
        let first = iter.next()?.opacity().get();
        if iter.all(|stop| (stop.opacity().get() - first).abs() <= 0.0001) {
            Some(first)
        } else {
            None
        }
    }

    fn gradient_stops_key(stops: &[Stop]) -> String {
        Self::normalized_gradient_stops_direction(stops, false)
            .into_iter()
            .map(|stop| {
                format!(
                    "{:.6}:{:.6}:{:.6}:{:.6}:{:.6}",
                    stop.offset, stop.r, stop.g, stop.b, stop.opacity
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    fn gradient_stops_key_direction(stops: &[Stop], reversed: bool) -> String {
        Self::normalized_gradient_stops_direction(stops, reversed)
            .into_iter()
            .map(|stop| {
                format!(
                    "{:.6}:{:.6}:{:.6}:{:.6}:{:.6}",
                    stop.offset, stop.r, stop.g, stop.b, stop.opacity
                )
            })
            .collect::<Vec<_>>()
            .join("|")
    }

    fn pdf_function_from_stops_direction(stops: &[Stop], reversed: bool) -> String {
        let stops = Self::normalized_gradient_stops_direction(stops, reversed);

        if stops.len() == 2 {
            return Self::type2_rgb_function(&stops[0], &stops[1]);
        }

        let functions = stops
            .windows(2)
            .map(|pair| Self::type2_rgb_function(&pair[0], &pair[1]))
            .collect::<Vec<_>>()
            .join(" ");
        let bounds = stops[1..stops.len() - 1]
            .iter()
            .map(|stop| format!("{:.6}", stop.offset))
            .collect::<Vec<_>>()
            .join(" ");
        let encode = (0..stops.len() - 1)
            .map(|_| "0 1".to_string())
            .collect::<Vec<_>>()
            .join(" ");

        format!(
            "<< /FunctionType 3 /Domain [0 1] /Functions [{}] /Bounds [{}] /Encode [{}] >>",
            functions, bounds, encode
        )
    }

    fn pdf_function_from_opacity_stops_direction(stops: &[Stop], reversed: bool) -> String {
        let stops = Self::normalized_gradient_stops_direction(stops, reversed);

        if stops.len() == 2 {
            return Self::type2_gray_function(&stops[0], &stops[1]);
        }

        let functions = stops
            .windows(2)
            .map(|pair| Self::type2_gray_function(&pair[0], &pair[1]))
            .collect::<Vec<_>>()
            .join(" ");
        let bounds = stops[1..stops.len() - 1]
            .iter()
            .map(|stop| format!("{:.6}", stop.offset))
            .collect::<Vec<_>>()
            .join(" ");
        let encode = (0..stops.len() - 1)
            .map(|_| "0 1".to_string())
            .collect::<Vec<_>>()
            .join(" ");

        format!(
            "<< /FunctionType 3 /Domain [0 1] /Functions [{}] /Bounds [{}] /Encode [{}] >>",
            functions, bounds, encode
        )
    }

    fn type2_rgb_function(start: &GradientStopData, end: &GradientStopData) -> String {
        format!(
            "<< /FunctionType 2 /Domain [0 1] /C0 [{:.6} {:.6} {:.6}] /C1 [{:.6} {:.6} {:.6}] /N 1 >>",
            start.r, start.g, start.b, end.r, end.g, end.b
        )
    }

    fn type2_gray_function(start: &GradientStopData, end: &GradientStopData) -> String {
        format!(
            "<< /FunctionType 2 /Domain [0 1] /C0 [{:.6}] /C1 [{:.6}] /N 1 >>",
            start.opacity, end.opacity
        )
    }

    pub(crate) fn pdf_matrix(transform: usvg::Transform) -> String {
        format!(
            "{:.6} {:.6} {:.6} {:.6} {:.6} {:.6}",
            transform.sx, transform.ky, transform.kx, transform.sy, transform.tx, transform.ty
        )
    }

    pub(crate) fn apply_stroke_properties(&mut self, stroke: &usvg::Stroke) {
        self.pdf_ops
            .push_str(&format!("{:.6} w ", stroke.width().get()));

        let cap = match stroke.linecap() {
            usvg::LineCap::Butt => 0,
            usvg::LineCap::Round => 1,
            usvg::LineCap::Square => 2,
        };
        self.pdf_ops.push_str(&format!("{} J ", cap));

        let join = match stroke.linejoin() {
            usvg::LineJoin::Miter | usvg::LineJoin::MiterClip => 0,
            usvg::LineJoin::Round => 1,
            usvg::LineJoin::Bevel => 2,
        };
        self.pdf_ops.push_str(&format!("{} j ", join));
        self.pdf_ops
            .push_str(&format!("{:.6} M ", stroke.miterlimit().get()));

        if let Some(dasharray) = stroke.dasharray() {
            if !dasharray.is_empty() {
                let dash_str = dasharray
                    .iter()
                    .map(|dash| format!("{:.6}", dash))
                    .collect::<Vec<_>>();
                self.pdf_ops.push_str(&format!(
                    "[{}] {:.6} d ",
                    dash_str.join(" "),
                    stroke.dashoffset()
                ));
            }
        }
    }

    pub(crate) fn blend_mode_name(blend_mode: usvg::BlendMode) -> &'static str {
        match blend_mode {
            usvg::BlendMode::Normal => "Normal",
            usvg::BlendMode::Multiply => "Multiply",
            usvg::BlendMode::Screen => "Screen",
            usvg::BlendMode::Overlay => "Overlay",
            usvg::BlendMode::Darken => "Darken",
            usvg::BlendMode::Lighten => "Lighten",
            usvg::BlendMode::ColorDodge => "ColorDodge",
            usvg::BlendMode::ColorBurn => "ColorBurn",
            usvg::BlendMode::HardLight => "HardLight",
            usvg::BlendMode::SoftLight => "SoftLight",
            usvg::BlendMode::Difference => "Difference",
            usvg::BlendMode::Exclusion => "Exclusion",
            usvg::BlendMode::Hue => "Hue",
            usvg::BlendMode::Saturation => "Saturation",
            usvg::BlendMode::Color => "Color",
            usvg::BlendMode::Luminosity => "Luminosity",
        }
    }

    pub(crate) fn gradient_is_natively_supported(
        _stops: &[Stop],
        spread_method: SpreadMethod,
    ) -> bool {
        matches!(
            spread_method,
            SpreadMethod::Pad | SpreadMethod::Reflect | SpreadMethod::Repeat
        )
    }

    fn spread_segment_reversed(spread: SpreadMethod, segment: i32) -> bool {
        matches!(spread, SpreadMethod::Reflect) && segment.rem_euclid(2) == 1
    }
}
