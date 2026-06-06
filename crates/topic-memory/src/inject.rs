//! HTML-comment bounded injection markers.

#[derive(Debug, Clone, Copy)]
pub struct MemorySectionMarkers {
    pub start: &'static str,
    pub end: &'static str,
}

pub const DEFAULT_MARKERS: MemorySectionMarkers = MemorySectionMarkers {
    start: "<!-- topic-memory-graph-start -->",
    end: "<!-- topic-memory-graph-end -->",
};

/// Insert or replace a bounded memory section in markdown.
#[must_use]
pub fn inject_memory_section(
    existing: &str,
    content: &str,
    markers: MemorySectionMarkers,
) -> String {
    let section = format!("{}\n{}\n{}", markers.start, content, markers.end);
    let si = existing.find(markers.start);
    let ei = existing.find(markers.end);
    if let (Some(si), Some(ei)) = (si, ei)
        && ei > si
    {
        let after_end = ei + markers.end.len();
        return format!("{}{}{}", &existing[..si], section, &existing[after_end..]);
    }
    let trimmed = existing.trim_end();
    if trimmed.is_empty() {
        return section;
    }
    format!("{trimmed}\n\n{section}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaces_existing_section() {
        let base = format!(
            "# Agents\n\n{}\nold\n{}\n",
            DEFAULT_MARKERS.start, DEFAULT_MARKERS.end
        );
        let out = inject_memory_section(&base, "new body", DEFAULT_MARKERS);
        assert!(out.contains("new body"));
        assert!(!out.contains("old"));
    }
}
