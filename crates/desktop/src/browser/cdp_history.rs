//! CDP navigation history (Windows WebView2): `Page.getNavigationHistory` + `navigateToHistoryEntry`.

use serde::Deserialize;

use super::cdp::call_devtools_protocol;
use super::{BrowserError, BrowserMode};
use tauri::AppHandle;

#[derive(Debug, Clone)]
pub struct CdpNavHistory {
    pub current_index: i64,
    pub entries: Vec<CdpHistoryEntry>,
}

#[derive(Debug, Clone)]
pub struct CdpHistoryEntry {
    pub id: i64,
    pub url: String,
    #[allow(dead_code)]
    pub title: String,
}

#[cfg(windows)]
pub fn is_available() -> bool {
    true
}

#[cfg(not(windows))]
pub fn is_available() -> bool {
    false
}

pub async fn fetch_navigation_history(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
) -> Result<CdpNavHistory, BrowserError> {
    #[cfg(not(windows))]
    {
        let _ = (app, mode, host_label);
        return Err(BrowserError::msg(
            "cdp_unsupported",
            "CDP history 仅 Windows WebView2 可用",
        ));
    }

    #[cfg(windows)]
    {
        let raw = call_devtools_protocol(app, mode, host_label, "Page.getNavigationHistory", "{}")
            .await?;
        parse_navigation_history(&raw).ok_or_else(|| {
            BrowserError::msg(
                "cdp_history_parse",
                "无法解析 Page.getNavigationHistory 响应",
            )
        })
    }
}

pub async fn navigate_history_entry(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
    entry_id: i64,
) -> Result<(), BrowserError> {
    #[cfg(not(windows))]
    {
        let _ = (app, mode, host_label, entry_id);
        return Err(BrowserError::msg(
            "cdp_unsupported",
            "CDP history 仅 Windows WebView2 可用",
        ));
    }

    #[cfg(windows)]
    {
        let params = format!(r#"{{"entryId":{entry_id}}}"#);
        call_devtools_protocol(
            app,
            mode,
            host_label,
            "Page.navigateToHistoryEntry",
            &params,
        )
        .await?;
        Ok(())
    }
}

/// Step back one entry; returns updated history when navigation was triggered.
pub async fn history_back(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
) -> Result<Option<CdpNavHistory>, BrowserError> {
    let hist = fetch_navigation_history(app, mode, host_label).await?;
    if hist.current_index <= 0 {
        return Ok(None);
    }
    let target = &hist.entries[hist.current_index as usize - 1];
    navigate_history_entry(app, mode, host_label, target.id).await?;
    Ok(Some(hist))
}

/// Step forward one entry; returns updated history when navigation was triggered.
pub async fn history_forward(
    app: &AppHandle,
    mode: BrowserMode,
    host_label: &str,
) -> Result<Option<CdpNavHistory>, BrowserError> {
    let hist = fetch_navigation_history(app, mode, host_label).await?;
    let next_idx = hist.current_index + 1;
    if next_idx as usize >= hist.entries.len() {
        return Ok(None);
    }
    let target = &hist.entries[next_idx as usize];
    navigate_history_entry(app, mode, host_label, target.id).await?;
    Ok(Some(hist))
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawHistory {
    #[serde(default)]
    current_index: i64,
    #[serde(default)]
    entries: Vec<RawEntry>,
}

#[derive(Deserialize)]
struct RawEntry {
    id: i64,
    url: String,
    #[serde(default)]
    title: String,
}

pub fn parse_navigation_history(raw: &str) -> Option<CdpNavHistory> {
    let parsed: RawHistory = serde_json::from_str(raw).ok()?;
    if parsed.entries.is_empty() {
        return None;
    }
    Some(CdpNavHistory {
        current_index: parsed.current_index,
        entries: parsed
            .entries
            .into_iter()
            .map(|e| CdpHistoryEntry {
                id: e.id,
                url: e.url,
                title: e.title,
            })
            .collect(),
    })
}

impl CdpNavHistory {
    pub fn can_go_back(&self) -> bool {
        self.current_index > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.current_index >= 0 && (self.current_index as usize + 1) < self.entries.len()
    }

    /// Mirror CDP entries into chrome-driven history vectors.
    pub fn to_chrome_history(&self) -> (Vec<String>, usize) {
        let urls: Vec<String> = self.entries.iter().map(|e| e.url.clone()).collect();
        let idx = self
            .current_index
            .clamp(0, urls.len().saturating_sub(1) as i64) as usize;
        (urls, idx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_navigation_history_basic() {
        let raw = r#"{"currentIndex":1,"entries":[
          {"id":1,"url":"https://a/","title":"A"},
          {"id":2,"url":"https://b/","title":"B"}
        ]}"#;
        let h = parse_navigation_history(raw).unwrap();
        assert_eq!(h.current_index, 1);
        assert_eq!(h.entries.len(), 2);
        assert!(h.can_go_back());
        assert!(!h.can_go_forward());
    }

    #[test]
    fn to_chrome_history_maps_index() {
        let h = parse_navigation_history(
            r#"{"currentIndex":0,"entries":[{"id":10,"url":"https://x/","title":""}]}"#,
        )
        .unwrap();
        let (urls, idx) = h.to_chrome_history();
        assert_eq!(urls, vec!["https://x/"]);
        assert_eq!(idx, 0);
    }
}
