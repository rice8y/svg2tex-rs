use super::{
    ExtGStateResource, FormResource, FunctionResource, ImageResource, PatternResource, PdfContext,
    PdfConverter, PdfResources, ShadingResource,
};

impl PdfConverter {
    pub(crate) fn ensure_ext_gstate_with_dicts(
        &mut self,
        key: String,
        pdf_dict: String,
        dvi_dict: String,
    ) -> String {
        if let Some(resource) = self.resources.ext_gstates.get(&key) {
            return resource.name.clone();
        }

        let name = format!("GS{}", self.resources.get_next_id());
        self.resources.ext_gstates.insert(
            key,
            ExtGStateResource {
                name: name.clone(),
                pdf_dict,
                dvi_dict,
            },
        );
        name
    }

    pub(crate) fn ensure_ext_gstate(&mut self, entries: &[String]) -> Option<String> {
        if entries.is_empty() {
            return None;
        }

        let key = entries.join(" ");
        let name = self.ensure_ext_gstate_with_dicts(
            key.clone(),
            format!("<</Type/ExtGState {}>>", entries.join(" ")),
            format!("<</Type/ExtGState {}>>", entries.join(" ")),
        );
        eprintln!("Created ExtGState: {} = <</Type/ExtGState {}>>", name, key);
        Some(name)
    }

    pub(crate) fn ensure_soft_mask_ext_gstate(&mut self, form_name: &str, subtype: &str) -> String {
        self.ensure_soft_mask_ext_gstate_with_transfer(form_name, subtype, None)
    }

    pub(crate) fn ensure_soft_mask_ext_gstate_with_transfer(
        &mut self,
        form_name: &str,
        subtype: &str,
        transfer_name: Option<&str>,
    ) -> String {
        let key = format!(
            "soft-mask/{subtype}/{form_name}/{}",
            transfer_name.unwrap_or("identity")
        );
        let pdf_transfer = transfer_name
            .map(|value| format!(" /TR {}", Self::tex_obj_ref(value)))
            .unwrap_or_default();
        let dvi_transfer = transfer_name
            .map(|value| format!(" /TR @{}", value))
            .unwrap_or_default();
        self.ensure_ext_gstate_with_dicts(
            key,
            format!(
                "<</Type/ExtGState /SMask <</S /{} /G {}{}>>>>",
                subtype,
                Self::tex_obj_ref(form_name),
                pdf_transfer
            ),
            format!(
                "<</Type/ExtGState /SMask <</S /{} /G @{}{}>>>>",
                subtype, form_name, dvi_transfer
            ),
        )
    }

    pub(crate) fn ensure_function(
        &mut self,
        key: String,
        pdf_dict: String,
        dvi_dict: String,
    ) -> String {
        if let Some(resource) = self.resources.functions.get(&key) {
            return resource.name.clone();
        }

        let name = format!("Fn{}", self.resources.get_next_id());
        self.resources.functions.insert(
            key,
            FunctionResource {
                name: name.clone(),
                pdf_dict,
                dvi_dict,
            },
        );
        name
    }

    pub(crate) fn sorted_functions(&self) -> Vec<(&str, &FunctionResource)> {
        let mut items = self
            .resources
            .functions
            .iter()
            .map(|(key, value)| (key.as_str(), value))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        items
    }

    pub(crate) fn sorted_ext_gstates(&self) -> Vec<(&str, &ExtGStateResource)> {
        let mut items = self
            .resources
            .ext_gstates
            .iter()
            .map(|(key, value)| (key.as_str(), value))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        items
    }

    pub(crate) fn sorted_images(&self) -> Vec<(&str, &ImageResource)> {
        let mut items = self
            .resources
            .images
            .iter()
            .map(|(name, resource)| (name.as_str(), resource))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.0.cmp(b.0));
        items
    }

    pub(crate) fn ensure_shading(&mut self, key: String, dict: String) -> String {
        if let Some(resource) = self.resources.shadings.get(&key) {
            return resource.name.clone();
        }

        let name = format!("Sh{}", self.resources.get_next_id());
        self.resources
            .shadings
            .insert(key, ShadingResource { name: name.clone(), dict });
        name
    }

    pub(crate) fn sorted_shadings(&self) -> Vec<(&str, &ShadingResource)> {
        let mut items = self
            .resources
            .shadings
            .iter()
            .map(|(key, value)| (key.as_str(), value))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        items
    }

    pub(crate) fn ensure_form(
        &mut self,
        key: String,
        pdf_dict: String,
        dvi_dict: String,
        stream: Vec<u8>,
    ) -> String {
        if let Some(resource) = self.resources.forms.get(&key) {
            return resource.name.clone();
        }

        let name = format!("Fm{}", self.resources.get_next_id());
        self.resources.forms.insert(
            key,
            FormResource {
                name: name.clone(),
                pdf_dict,
                dvi_dict,
                stream,
            },
        );
        name
    }

    pub(crate) fn sorted_forms(&self) -> Vec<(&str, &FormResource)> {
        let mut items = self
            .resources
            .forms
            .iter()
            .map(|(key, value)| (key.as_str(), value))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        items
    }

    pub(crate) fn ensure_pattern(
        &mut self,
        key: String,
        pdf_dict: String,
        dvi_dict: String,
        stream: Vec<u8>,
    ) -> String {
        if let Some(resource) = self.resources.patterns.get(&key) {
            return resource.name.clone();
        }

        let name = format!("Pt{}", self.resources.get_next_id());
        self.resources.patterns.insert(
            key,
            PatternResource {
                name: name.clone(),
                pdf_dict,
                dvi_dict,
                stream,
            },
        );
        name
    }

    pub(crate) fn sorted_patterns(&self) -> Vec<(&str, &PatternResource)> {
        let mut items = self
            .resources
            .patterns
            .iter()
            .map(|(key, value)| (key.as_str(), value))
            .collect::<Vec<_>>();
        items.sort_by(|a, b| a.1.name.cmp(&b.1.name));
        items
    }

    pub(crate) fn inline_pdf_resource_dict(&self, include_patterns: bool) -> String {
        let mut sections = Vec::new();

        let ext_gstates = self
            .sorted_ext_gstates()
            .into_iter()
            .map(|(_, resource)| format!("/{} {}", resource.name, Self::tex_obj_ref(&resource.name)))
            .collect::<Vec<_>>();
        if !ext_gstates.is_empty() {
            sections.push(format!("/ExtGState<<{}>>", ext_gstates.join(" ")));
        }

        let shadings = self
            .sorted_shadings()
            .into_iter()
            .map(|(_, shading)| format!("/{} {}", shading.name, Self::tex_obj_ref(&shading.name)))
            .collect::<Vec<_>>();
        if !shadings.is_empty() {
            sections.push(format!("/Shading<<{}>>", shadings.join(" ")));
        }

        if include_patterns {
            let patterns = self
                .sorted_patterns()
                .into_iter()
                .map(|(_, pattern)| format!("/{} {}", pattern.name, Self::tex_obj_ref(&pattern.name)))
                .collect::<Vec<_>>();
            if !patterns.is_empty() {
                sections.push(format!("/Pattern<<{}>>", patterns.join(" ")));
            }
        }

        let xobjects = self
            .sorted_images()
            .into_iter()
            .map(|(img_name, _)| format!("/{} {}", img_name, Self::tex_obj_ref(img_name)))
            .chain(
                self.sorted_forms()
                    .into_iter()
                    .map(|(_, form)| format!("/{} {}", form.name, Self::tex_obj_ref(&form.name))),
            )
            .collect::<Vec<_>>();
        if !xobjects.is_empty() {
            sections.push(format!("/XObject<<{}>>", xobjects.join(" ")));
        }

        if sections.is_empty() {
            String::new()
        } else {
            format!("<<{}>>", sections.join(" "))
        }
    }

    pub(crate) fn inline_dvi_resource_dict(&self, include_patterns: bool) -> String {
        let mut sections = Vec::new();

        let ext_gstates = self
            .sorted_ext_gstates()
            .into_iter()
            .map(|(_, resource)| format!("/{} @{}", resource.name, resource.name))
            .collect::<Vec<_>>();
        if !ext_gstates.is_empty() {
            sections.push(format!("/ExtGState<<{}>>", ext_gstates.join(" ")));
        }

        let shadings = self
            .sorted_shadings()
            .into_iter()
            .map(|(_, shading)| format!("/{} @{}", shading.name, shading.name))
            .collect::<Vec<_>>();
        if !shadings.is_empty() {
            sections.push(format!("/Shading<<{}>>", shadings.join(" ")));
        }

        if include_patterns {
            let patterns = self
                .sorted_patterns()
                .into_iter()
                .map(|(_, pattern)| format!("/{} @{}", pattern.name, pattern.name))
                .collect::<Vec<_>>();
            if !patterns.is_empty() {
                sections.push(format!("/Pattern<<{}>>", patterns.join(" ")));
            }
        }

        let xobjects = self
            .sorted_images()
            .into_iter()
            .map(|(img_name, _)| format!("/{} @{}", img_name, img_name))
            .chain(
                self.sorted_forms()
                    .into_iter()
                    .map(|(_, form)| format!("/{} @{}", form.name, form.name)),
            )
            .collect::<Vec<_>>();
        if !xobjects.is_empty() {
            sections.push(format!("/XObject<<{}>>", xobjects.join(" ")));
        }

        if sections.is_empty() {
            String::new()
        } else {
            format!("<<{}>>", sections.join(" "))
        }
    }
}

impl PdfResources {
    pub(crate) fn new() -> Self {
        Self {
            ext_gstates: std::collections::HashMap::new(),
            functions: std::collections::HashMap::new(),
            images: std::collections::HashMap::new(),
            shadings: std::collections::HashMap::new(),
            forms: std::collections::HashMap::new(),
            patterns: std::collections::HashMap::new(),
            next_id: 1,
        }
    }

    pub(crate) fn get_next_id(&mut self) -> usize {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

impl PdfContext {
    pub(crate) fn new() -> Self {
        Self {
            current_point: None,
            subpath_start: None,
        }
    }
}
