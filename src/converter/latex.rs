use super::{ImageResource, PdfConverter};
use crate::{TexEngine, TexFormat};

impl PdfConverter {
    pub(crate) fn generate_latex(&self) -> String {
        let mut output = String::new();

        match self.tex_format {
            TexFormat::Standalone => output.push_str("\\documentclass[crop]{standalone}\n"),
            TexFormat::Article => output.push_str("\\documentclass{article}\n"),
            TexFormat::Snippet => {}
        }
        match self.tex_format {
            TexFormat::Standalone | TexFormat::Article => {
                match self.engine {
                    TexEngine::Auto => {
                        output.push_str("\\usepackage{iftex}\n");
                        output.push_str("\\usepackage{graphicx}\n");
                        output.push_str("\\usepackage{calc}\n");
                        output.push_str("\\makeatletter\n");
                        output.push_str("\\newif\\ifsvgpdfmode\n");
                        output.push_str("\\@ifundefined{pdfoutput}{\\svgpdfmodefalse}{%\n");
                        output.push_str(
                            "  \\ifnum\\pdfoutput>0 \\svgpdfmodetrue\\else\\svgpdfmodefalse\\fi\n",
                        );
                        output.push_str("}\n");
                        output.push_str("\\makeatother\n");
                    }
                    TexEngine::PdfTeX | TexEngine::LuaTeX => {
                        output.push_str("\\usepackage{graphicx}\n");
                        output.push_str("\\usepackage{calc}\n");
                    }
                    TexEngine::XeTeX | TexEngine::PTeX | TexEngine::UpTeX => {
                        output.push_str("\\usepackage{graphicx}\n");
                    }
                }
                output.push('\n');
            }
            TexFormat::Snippet => {
                output.push_str("\\RequirePackage{iftex}\n");
                output.push_str("\\RequirePackage{graphicx}\n");
                output.push_str("\\RequirePackage{calc}\n");
                output.push_str("\\makeatletter\n");
                output.push_str("\\newif\\ifsvgpdfmode\n");
                output.push_str("\\@ifundefined{pdfoutput}{\\svgpdfmodefalse}{%\n");
                output.push_str("  \\ifnum\\pdfoutput>0 \\svgpdfmodetrue\\else\\svgpdfmodefalse\\fi\n");
                output.push_str("}\n");
                output.push_str("\\makeatother\n");
                output.push('\n');
            }
        }

        let pdftex_resource_defs = self.generate_pdftex_resource_defs();
        let luatex_resource_defs = self.generate_lua_resource_defs();
        let dvi_resource_defs = self.generate_dvi_resource_defs();

        if !pdftex_resource_defs.is_empty()
            || !luatex_resource_defs.is_empty()
            || !dvi_resource_defs.is_empty()
        {
            output.push_str("% PDF Resource Definitions\n");
            match self.engine {
                TexEngine::Auto => {
                    output.push_str("\\ifsvgpdfmode\n");
                    output.push_str("  \\ifpdftex\n");
                    output.push_str(&pdftex_resource_defs);
                    output.push_str("  \\else\\ifluatex\n");
                    output.push_str(&luatex_resource_defs);
                    output.push_str("  \\else\n");
                    output.push_str(&dvi_resource_defs);
                    output.push_str("  \\fi\\fi\n");
                    output.push_str("\\else\n");
                    output.push_str(&dvi_resource_defs);
                    output.push_str("\\fi\n");
                }
                TexEngine::PdfTeX => output.push_str(&pdftex_resource_defs),
                TexEngine::LuaTeX => output.push_str(&luatex_resource_defs),
                TexEngine::XeTeX | TexEngine::PTeX | TexEngine::UpTeX => {
                    output.push_str(&dvi_resource_defs)
                }
            }
            output.push('\n');
        }

        let pdf_page_resources = self.build_pdf_page_resources();
        let dvi_page_resources = self.generate_dvi_page_resources();
        if !pdf_page_resources.is_empty() || !dvi_page_resources.is_empty() {
            output.push_str("% Add resources to page\n");
            match self.engine {
                TexEngine::Auto => {
                    output.push_str("\\ifsvgpdfmode\n");
                    output.push_str("  \\ifpdftex\n");
                    if !pdf_page_resources.is_empty() {
                        output.push_str("    \\pdfpageresources\\expanded{{\n");
                        output.push_str(&pdf_page_resources);
                        output.push_str("    }}\n");
                    }
                    output.push_str("  \\else\\ifluatex\n");
                    if !pdf_page_resources.is_empty() {
                        output.push_str("    \\pdfvariable pageresources\\expanded{{\n");
                        output.push_str(&pdf_page_resources);
                        output.push_str("    }}\n");
                    }
                    output.push_str("  \\else\n");
                    output.push_str(&dvi_page_resources);
                    output.push_str("  \\fi\\fi\n");
                    output.push_str("\\else\n");
                    output.push_str(&dvi_page_resources);
                    output.push_str("\\fi\n");
                }
                TexEngine::PdfTeX => {
                    if !pdf_page_resources.is_empty() {
                        output.push_str("\\pdfpageresources\\expanded{{\n");
                        output.push_str(&pdf_page_resources);
                        output.push_str("}}\n");
                    }
                }
                TexEngine::LuaTeX => {
                    if !pdf_page_resources.is_empty() {
                        output.push_str("\\pdfvariable pageresources\\expanded{{\n");
                        output.push_str(&pdf_page_resources);
                        output.push_str("}}\n");
                    }
                }
                TexEngine::XeTeX | TexEngine::PTeX | TexEngine::UpTeX => {
                    output.push_str(&dvi_page_resources);
                }
            }
            output.push('\n');
        }

        output.push_str("% Definition of SVG Command\n");
        output.push_str("\\newcommand{\\mysvg}[1][1]{%\n");
        output.push_str("  \\scalebox{#1}{%\n");
        output.push_str(&format!(
            "    \\begin{{picture}}({},{})%\n",
            self.size.width(),
            self.size.height()
        ));

        match self.engine {
            TexEngine::Auto => {
                output.push_str("      \\ifsvgpdfmode\n");
                output.push_str("        \\ifpdftex\n");
                output.push_str(&format!(
                    "          \\pdfliteral direct{{{}}}%\n",
                    self.pdf_ops
                ));
                output.push_str("        \\else\\ifluatex\n");
                output.push_str(&format!(
                    "          \\pdfextension literal direct{{{}}}%\n",
                    self.pdf_ops
                ));
                output.push_str("        \\else\n");
                output.push_str(&format!(
                    "          \\special{{pdf:literal direct {}}}%\n",
                    self.pdf_ops
                ));
                output.push_str("        \\fi\\fi\n");
                output.push_str("      \\else\n");
                output.push_str(&format!(
                    "        \\special{{pdf:literal direct {}}}%\n",
                    self.pdf_ops
                ));
                output.push_str("      \\fi\n");
            }
            TexEngine::PdfTeX => {
                output.push_str(&format!(
                    "      \\pdfliteral direct{{{}}}%\n",
                    self.pdf_ops
                ));
            }
            TexEngine::LuaTeX => {
                output.push_str(&format!(
                    "      \\pdfextension literal direct{{{}}}%\n",
                    self.pdf_ops
                ));
            }
            TexEngine::XeTeX | TexEngine::PTeX | TexEngine::UpTeX => {
                output.push_str(&format!(
                    "      \\special{{pdf:literal direct {}}}%\n",
                    self.pdf_ops
                ));
            }
        }

        output.push_str("    \\end{picture}%\n");
        output.push_str("  }%\n");
        output.push_str("}\n\n");
        match self.tex_format {
            TexFormat::Standalone | TexFormat::Article => {
                output.push_str("\\begin{document}\n\n");
                output.push_str("\\mysvg\n\n");
                output.push_str("\\end{document}\n");
            }
            TexFormat::Snippet => {}
        }

        eprintln!("\n=== Conversion Summary ===");
        eprintln!("Target engine: {:?}", self.engine);
        eprintln!("Resources created:");
        eprintln!("  ExtGState objects: {}", self.resources.ext_gstates.len());
        eprintln!("  Function objects: {}", self.resources.functions.len());
        eprintln!("  Shading objects: {}", self.resources.shadings.len());
        eprintln!("  Form objects: {}", self.resources.forms.len());
        eprintln!("  Pattern objects: {}", self.resources.patterns.len());
        eprintln!("  Image objects: {}", self.resources.images.len());

        output
    }

    fn generate_pdftex_resource_defs(&self) -> String {
        let mut output = String::new();

        for (_, function) in self.sorted_functions() {
            output.push_str("\\pdfobj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\pdflastobj}}\n",
                function.name
            ));
        }

        for (_, shading) in self.sorted_shadings() {
            output.push_str("\\pdfobj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\pdflastobj}}\n",
                shading.name
            ));
        }

        for (_, form) in self.sorted_forms() {
            output.push_str("\\pdfobj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\pdflastobj}}\n",
                form.name
            ));
        }

        for (_, pattern) in self.sorted_patterns() {
            output.push_str("\\pdfobj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\pdflastobj}}\n",
                pattern.name
            ));
        }

        for (_, resource) in self.sorted_ext_gstates() {
            output.push_str("\\pdfobj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\pdflastobj}}\n",
                resource.name
            ));
        }

        for (img_name, resource) in self.sorted_images() {
            if let Some(smask) = &resource.smask {
                output.push_str("\\pdfobj reserveobjnum\n");
                output.push_str(&format!(
                    "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\pdflastobj}}\n",
                    smask.name
                ));
            }
            output.push_str("\\pdfobj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\pdflastobj}}\n",
                img_name
            ));
        }

        for (_, function) in self.sorted_functions() {
            output.push_str(&format!(
                "\\immediate\\pdfobj useobjnum {}{{{}}}\n",
                Self::tex_obj_macro(&function.name),
                function.pdf_dict
            ));
        }

        for (_, shading) in self.sorted_shadings() {
            output.push_str(&format!(
                "\\immediate\\pdfobj useobjnum {}{{{}}}\n",
                Self::tex_obj_macro(&shading.name),
                shading.dict
            ));
        }

        for (_, form) in self.sorted_forms() {
            output.push_str(&format!(
                "\\immediate\\pdfobj useobjnum {} stream attr{{{}}}{{{}}}\\relax\n",
                Self::tex_obj_macro(&form.name),
                form.pdf_dict,
                Self::ascii_hex_stream(&form.stream)
            ));
        }

        for (_, pattern) in self.sorted_patterns() {
            output.push_str(&format!(
                "\\immediate\\pdfobj useobjnum {} stream attr{{{}}}{{{}}}\\relax\n",
                Self::tex_obj_macro(&pattern.name),
                pattern.pdf_dict,
                Self::ascii_hex_stream(&pattern.stream)
            ));
        }

        for (_, resource) in self.sorted_ext_gstates() {
            output.push_str(&format!(
                "\\immediate\\pdfobj useobjnum {}{{{}}}\n",
                Self::tex_obj_macro(&resource.name),
                resource.pdf_dict
            ));
        }

        for (img_name, resource) in self.sorted_images() {
            output.push_str(&self.generate_pdftex_image_object(img_name, resource));
        }

        output
    }

    fn generate_lua_resource_defs(&self) -> String {
        let mut output = String::new();

        for (_, function) in self.sorted_functions() {
            output.push_str("\\pdfextension obj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\numexpr\\pdffeedback lastobj\\relax}}\n",
                function.name
            ));
        }

        for (_, shading) in self.sorted_shadings() {
            output.push_str("\\pdfextension obj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\numexpr\\pdffeedback lastobj\\relax}}\n",
                shading.name
            ));
        }

        for (_, form) in self.sorted_forms() {
            output.push_str("\\pdfextension obj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\numexpr\\pdffeedback lastobj\\relax}}\n",
                form.name
            ));
        }

        for (_, pattern) in self.sorted_patterns() {
            output.push_str("\\pdfextension obj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\numexpr\\pdffeedback lastobj\\relax}}\n",
                pattern.name
            ));
        }

        for (_, resource) in self.sorted_ext_gstates() {
            output.push_str("\\pdfextension obj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\numexpr\\pdffeedback lastobj\\relax}}\n",
                resource.name
            ));
        }

        for (img_name, resource) in self.sorted_images() {
            if let Some(smask) = &resource.smask {
                output.push_str("\\pdfextension obj reserveobjnum\n");
                output.push_str(&format!(
                    "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\numexpr\\pdffeedback lastobj\\relax}}\n",
                    smask.name
                ));
            }
            output.push_str("\\pdfextension obj reserveobjnum\n");
            output.push_str(&format!(
                "\\expandafter\\edef\\csname svgobj@{}\\endcsname{{\\the\\numexpr\\pdffeedback lastobj\\relax}}\n",
                img_name
            ));
        }

        for (_, function) in self.sorted_functions() {
            output.push_str(&format!(
                "\\immediate\\pdfextension obj useobjnum {}{{{}}}\n",
                Self::tex_obj_macro(&function.name),
                function.pdf_dict
            ));
        }

        for (_, shading) in self.sorted_shadings() {
            output.push_str(&format!(
                "\\immediate\\pdfextension obj useobjnum {}{{{}}}\n",
                Self::tex_obj_macro(&shading.name),
                shading.dict
            ));
        }

        for (_, form) in self.sorted_forms() {
            output.push_str(&format!(
                "\\immediate\\pdfextension obj useobjnum {} stream attr{{{}}}{{{}}}\\relax\n",
                Self::tex_obj_macro(&form.name),
                form.pdf_dict,
                Self::ascii_hex_stream(&form.stream)
            ));
        }

        for (_, pattern) in self.sorted_patterns() {
            output.push_str(&format!(
                "\\immediate\\pdfextension obj useobjnum {} stream attr{{{}}}{{{}}}\\relax\n",
                Self::tex_obj_macro(&pattern.name),
                pattern.pdf_dict,
                Self::ascii_hex_stream(&pattern.stream)
            ));
        }

        for (_, resource) in self.sorted_ext_gstates() {
            output.push_str(&format!(
                "\\immediate\\pdfextension obj useobjnum {}{{{}}}\n",
                Self::tex_obj_macro(&resource.name),
                resource.pdf_dict
            ));
        }

        for (img_name, resource) in self.sorted_images() {
            output.push_str(&self.generate_lua_image_object(img_name, resource));
        }

        output
    }

    fn generate_dvi_resource_defs(&self) -> String {
        let mut output = String::new();

        for (_, function) in self.sorted_functions() {
            output.push_str(&format!(
                "\\special{{pdf:obj @{} {}}}\n",
                function.name, function.dvi_dict
            ));
        }

        for (_, shading) in self.sorted_shadings() {
            output.push_str(&format!(
                "\\special{{pdf:obj @{} {}}}\n",
                shading.name, shading.dict
            ));
        }

        for (_, form) in self.sorted_forms() {
            output.push_str(&format!(
                "\\special{{pdf:stream @{} <{}> {}}}\n",
                form.name,
                Self::hex_stream(&form.stream),
                form.dvi_dict
            ));
        }

        for (_, pattern) in self.sorted_patterns() {
            output.push_str(&format!(
                "\\special{{pdf:stream @{} <{}> {}}}\n",
                pattern.name,
                Self::hex_stream(&pattern.stream),
                pattern.dvi_dict
            ));
        }

        for (_, resource) in self.sorted_ext_gstates() {
            output.push_str(&format!(
                "\\special{{pdf:obj @{} {}}}\n",
                resource.name, resource.dvi_dict
            ));
        }

        for (img_name, resource) in self.sorted_images() {
            output.push_str(&self.generate_dvi_image_object(img_name, resource));
        }

        output
    }

    pub(crate) fn build_pdf_page_resources(&self) -> String {
        let mut sections = Vec::new();

        let ext_gstates = self
            .sorted_ext_gstates()
            .into_iter()
            .map(|(_, resource)| format!("/{} {}", resource.name, Self::tex_obj_ref(&resource.name)))
            .collect::<Vec<_>>();
        if !ext_gstates.is_empty() {
            sections.push(format!("  /ExtGState<<{}>>\n", ext_gstates.join(" ")));
        }

        let shadings = self
            .sorted_shadings()
            .into_iter()
            .map(|(_, shading)| format!("/{} {}", shading.name, Self::tex_obj_ref(&shading.name)))
            .collect::<Vec<_>>();
        if !shadings.is_empty() {
            sections.push(format!("  /Shading<<{}>>\n", shadings.join(" ")));
        }

        let patterns = self
            .sorted_patterns()
            .into_iter()
            .map(|(_, pattern)| format!("/{} {}", pattern.name, Self::tex_obj_ref(&pattern.name)))
            .collect::<Vec<_>>();
        if !patterns.is_empty() {
            sections.push(format!("  /Pattern<<{}>>\n", patterns.join(" ")));
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
            sections.push(format!("  /XObject<<{}>>\n", xobjects.join(" ")));
        }

        sections.concat()
    }

    fn generate_dvi_page_resources(&self) -> String {
        let mut resources = Vec::new();

        let ext_gstates = self
            .sorted_ext_gstates()
            .into_iter()
            .map(|(_, resource)| format!("/{} @{}", resource.name, resource.name))
            .collect::<Vec<_>>();
        if !ext_gstates.is_empty() {
            resources.push(format!("/ExtGState<<{}>>", ext_gstates.join(" ")));
        }

        let shadings = self
            .sorted_shadings()
            .into_iter()
            .map(|(_, shading)| format!("/{} @{}", shading.name, shading.name))
            .collect::<Vec<_>>();
        if !shadings.is_empty() {
            resources.push(format!("/Shading<<{}>>", shadings.join(" ")));
        }

        let patterns = self
            .sorted_patterns()
            .into_iter()
            .map(|(_, pattern)| format!("/{} @{}", pattern.name, pattern.name))
            .collect::<Vec<_>>();
        if !patterns.is_empty() {
            resources.push(format!("/Pattern<<{}>>", patterns.join(" ")));
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
            resources.push(format!("/XObject<<{}>>", xobjects.join(" ")));
        }

        let resources = if resources.is_empty() {
            String::new()
        } else {
            format!("<<{}>>", resources.join(" "))
        };
        if resources.is_empty() {
            String::new()
        } else {
            format!("\\special{{pdf:put @resources {}}}\n", resources)
        }
    }

    fn generate_pdftex_image_object(&self, img_name: &str, resource: &ImageResource) -> String {
        let mut output = String::new();

        if let Some(smask) = &resource.smask {
            output.push_str(&format!(
                "\\immediate\\pdfobj useobjnum {} stream attr{{{}}}{{{}}}\\relax\n",
                Self::tex_obj_macro(&smask.name),
                self.tex_image_dict_for_pdftex_smask(smask),
                Self::ascii_hex_stream(&smask.data)
            ));
        }

        output.push_str(&format!(
            "\\immediate\\pdfobj useobjnum {} stream attr{{{}}}{{{}}}\\relax\n",
            Self::tex_obj_macro(img_name),
            self.tex_image_dict_for_pdftex(img_name, resource),
            Self::ascii_hex_stream(&resource.data)
        ));

        output
    }

    fn generate_lua_image_object(&self, img_name: &str, resource: &ImageResource) -> String {
        let mut output = String::new();

        if let Some(smask) = &resource.smask {
            output.push_str(&format!(
                "\\immediate\\pdfextension obj useobjnum {} stream attr{{{}}}{{{}}}\\relax\n",
                Self::tex_obj_macro(&smask.name),
                self.tex_image_dict_for_lua_smask(smask),
                Self::ascii_hex_stream(&smask.data)
            ));
        }

        output.push_str(&format!(
            "\\immediate\\pdfextension obj useobjnum {} stream attr{{{}}}{{{}}}\\relax\n",
            Self::tex_obj_macro(img_name),
            self.tex_image_dict_for_lua(img_name, resource),
            Self::ascii_hex_stream(&resource.data)
        ));

        output
    }

    fn generate_dvi_image_object(&self, img_name: &str, resource: &ImageResource) -> String {
        let mut output = String::new();

        if let Some(smask) = &resource.smask {
            output.push_str(&format!(
                "\\special{{pdf:stream @{} <{}> {}}}\n",
                smask.name,
                Self::hex_stream(&smask.data),
                self.dvi_image_dict_for_smask(smask)
            ));
        }

        output.push_str(&format!(
            "\\special{{pdf:stream @{} <{}> {}}}\n",
            img_name,
            Self::hex_stream(&resource.data),
            self.dvi_image_dict(img_name, resource)
        ));

        output
    }
}
