/// TeX backends supported by the generated output wrapper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexEngine {
    Auto,
    PdfTeX,
    LuaTeX,
    XeTeX,
    PTeX,
    UpTeX,
}

impl std::str::FromStr for TexEngine {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "auto" => Ok(TexEngine::Auto),
            "pdftex" => Ok(TexEngine::PdfTeX),
            "luatex" => Ok(TexEngine::LuaTeX),
            "xetex" => Ok(TexEngine::XeTeX),
            "ptex" => Ok(TexEngine::PTeX),
            "uptex" => Ok(TexEngine::UpTeX),
            _ => Err(format!("Unknown engine: {}", s)),
        }
    }
}
