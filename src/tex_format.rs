#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TexFormat {
    Standalone,
    Article,
    Snippet,
}

impl std::str::FromStr for TexFormat {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "standalone" => Ok(TexFormat::Standalone),
            "article" => Ok(TexFormat::Article),
            "snippet" => Ok(TexFormat::Snippet),
            _ => Err(format!("Unknown TeX format: {}", s)),
        }
    }
}
