//! PR review prompt formatting (retained for unit tests).

#[derive(Debug, Clone, Default)]
pub(crate) struct GhPullRequest {
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) base: String,
    pub(crate) head: String,
    pub(crate) url: String,
}

pub(crate) fn format_pr_prompt(number: u32, view: &GhPullRequest, diff: &str) -> String {
    const MAX_DIFF_BYTES: usize = 200 * 1024;
    let diff_section = if diff.len() > MAX_DIFF_BYTES {
        let cut = (0..=MAX_DIFF_BYTES)
            .rev()
            .find(|&i| diff.is_char_boundary(i))
            .unwrap_or(0);
        format!(
            "{}\n\n[…diff truncated at {} KiB; ask me to fetch more if needed]\n",
            &diff[..cut],
            MAX_DIFF_BYTES / 1024
        )
    } else {
        diff.to_string()
    };
    let body = if view.body.trim().is_empty() {
        "(no description)".to_string()
    } else {
        view.body.trim().to_string()
    };
    let title = if view.title.trim().is_empty() {
        format!("(PR #{number})")
    } else {
        view.title.trim().to_string()
    };
    let branches = match (view.base.is_empty(), view.head.is_empty()) {
        (false, false) => format!("{} ← {}", view.base, view.head),
        (false, true) => view.base.clone(),
        (true, false) => view.head.clone(),
        _ => "(unknown)".to_string(),
    };
    format!(
        "Review PR #{number} — {title}\n\
         \n\
         URL: {url}\n\
         Branches: {branches}\n\
         \n\
         ## Description\n\
         \n\
         {body}\n\
         \n\
         ## Diff\n\
         \n\
         ```diff\n\
         {diff_section}\n\
         ```\n",
        url = if view.url.is_empty() {
            "(unavailable)"
        } else {
            view.url.as_str()
        },
    )
}
