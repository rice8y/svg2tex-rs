/// Rewrites specific SVG constructs into a form `usvg` and the converter can
/// handle more predictably.
///
/// At the moment this only rewrites `clipPath` elements that contain raster
/// images into alpha masks so the resulting PDF semantics stay intact.
pub(crate) fn preprocess_svg(svg_data: &[u8]) -> Vec<u8> {
    let Ok(source) = std::str::from_utf8(svg_data) else {
        return svg_data.to_vec();
    };

    let Ok(doc) = roxmltree::Document::parse(source) else {
        return svg_data.to_vec();
    };

    let mut clip_paths = Vec::new();
    for node in doc
        .descendants()
        .filter(|node| node.has_tag_name("clipPath"))
    {
        let Some(id) = node.attribute("id") else {
            continue;
        };
        if !node
            .descendants()
            .any(|child| child.is_element() && child.tag_name().name() == "image")
        {
            continue;
        }

        clip_paths.push((id.to_string(), node.range()));
    }

    if clip_paths.is_empty() {
        return svg_data.to_vec();
    }

    let mut rewritten = source.to_string();

    // Rewrite from the end of the document so byte ranges collected from the
    // original parse stay valid as we replace earlier elements.
    clip_paths.sort_by(|a, b| b.1.start.cmp(&a.1.start));
    for (_, range) in &clip_paths {
        let element = &rewritten[range.start..range.end];
        let Some(open_end) = element.find('>') else {
            continue;
        };
        let Some(close_start) = element.rfind("</clipPath>") else {
            continue;
        };

        let opening = &element[..=open_end];
        let inner = &element[open_end + 1..close_start];
        let mut new_opening = opening.replacen(
            "<clipPath",
            "<mask mask-type=\"alpha\" maskUnits=\"userSpaceOnUse\" maskContentUnits=\"userSpaceOnUse\"",
            1,
        );
        new_opening = new_opening.replacen("<clipPath:", "<mask:", 1);
        let replacement = format!("{new_opening}{inner}</mask>");
        rewritten.replace_range(range.clone(), &replacement);
    }

    for (id, _) in &clip_paths {
        rewritten = rewritten.replace(
            &format!("clip-path=\"url(#{id})\""),
            &format!("mask=\"url(#{id})\""),
        );
        rewritten = rewritten.replace(
            &format!("clip-path='url(#{id})'"),
            &format!("mask='url(#{id})'"),
        );
        rewritten = rewritten.replace(
            &format!("clip-path:url(#{id})"),
            &format!("mask:url(#{id})"),
        );
    }

    rewritten.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::preprocess_svg;

    #[test]
    fn rewrites_clip_path_images_into_masks() {
        let source = br#"<svg xmlns="http://www.w3.org/2000/svg">
<defs><clipPath id="c"><image href="x"/></clipPath></defs>
<rect clip-path="url(#c)"/>
</svg>"#;
        let rewritten = String::from_utf8(preprocess_svg(source)).unwrap();
        assert!(rewritten.contains("<mask mask-type=\"alpha\""));
        assert!(rewritten.contains("</mask>"));
        assert!(rewritten.contains("mask=\"url(#c)\""));
    }
}
