---
name: "multi-search-engine"
description: "Multi search engine integration with 16 engines (7 CN + 9 Global). Supports advanced search operators, time filters, site search, privacy engines, and WolframAlpha knowledge queries. No API keys required."
---

# Multi Search Engine

Integration of 16 search engines for web research without API keys. Uses Zagens `fetch_url` (not `web_fetch`).

## Workflow

1. **Prefer structured tools first**: Use `web_search` (or `web.run` with `search_query`) when a single backend is enough. Load this skill when you need multi-engine coverage or CN-specific sources (Baidu, Sogou WeChat, etc.).

2. **Language routing**: Chinese queries → domestic engines (Baidu, Bing CN/INT, 360, Sogou, WeChat, Shenma). Non-Chinese → international engines (Google, DDG, Brave, Startpage, …). Pick 2–3 engines per task, not all 16.

3. **Controlled search**: Call `fetch_url` on engine search URLs with rate limiting:
   - 1–2 second gap between requests
   - Batch 2–3 engines per round
   - On 403/429, try `cn.bing.com` or another engine instead of retry loops

4. **Mandatory second hop**: Search result pages are HTML noise — **always** `fetch_url` the 2–3 most relevant result URLs from any engine before writing conclusions. Snippets alone are not evidence.

5. **Result aggregation**: Merge titles/URLs across engines, dedupe, then fetch top pages for full text.

## Search Engines

### Domestic (7)
- **Baidu**: `https://www.baidu.com/s?wd={keyword}`
- **Bing CN**: `https://cn.bing.com/search?q={keyword}&ensearch=0`
- **Bing INT**: `https://cn.bing.com/search?q={keyword}&ensearch=1`
- **360**: `https://www.so.com/s?q={keyword}`
- **Sogou**: `https://sogou.com/web?query={keyword}`
- **WeChat**: `https://wx.sogou.com/weixin?type=2&query={keyword}`
- **Shenma**: `https://m.sm.cn/s?q={keyword}`

### International (9)
- **Google**: `https://www.google.com/search?q={keyword}`
- **Google HK**: `https://www.google.com.hk/search?q={keyword}`
- **DuckDuckGo**: `https://duckduckgo.com/html/?q={keyword}`
- **Yahoo**: `https://search.yahoo.com/search?p={keyword}`
- **Startpage**: `https://www.startpage.com/sp/search?query={keyword}`
- **Brave**: `https://search.brave.com/search?q={keyword}`
- **Ecosia**: `https://www.ecosia.org/search?q={keyword}`
- **Qwant**: `https://www.qwant.com/?q={keyword}`
- **WolframAlpha**: `https://www.wolframalpha.com/input?i={keyword}`

## Quick Examples

```json
{"url": "https://cn.bing.com/search?q=python+tutorial&ensearch=0"}
{"url": "https://www.baidu.com/s?wd=竞品分析"}
{"url": "https://wx.sogou.com/weixin?type=2&query=机器之心"}
{"url": "https://duckduckgo.com/html/?q=privacy+tools"}
```

Use the `fetch_url` tool with each URL. After identifying result links, fetch those pages too.

## Advanced Operators

| Operator | Example | Description |
|----------|---------|-------------|
| `site:` | `site:github.com python` | Search within site |
| `filetype:` | `filetype:pdf report` | Specific file type |
| `""` | `"machine learning"` | Exact match |
| `-` | `python -snake` | Exclude term |
| `OR` | `cat OR dog` | Either term |

## Time Filters (Google-style)

| Parameter | Description |
|-----------|-------------|
| `tbs=qdr:h` | Past hour |
| `tbs=qdr:d` | Past day |
| `tbs=qdr:w` | Past week |
| `tbs=qdr:m` | Past month |
| `tbs=qdr:y` | Past year |

## Documentation

- `references/advanced-search.md` - Domestic search guide
- `references/international-search.md` - International search guide

## Limitations (Zagens runtime)

- `fetch_url` is stateless — no cookie jar between calls; Baidu/Google may return 403 or captcha pages.
- Prefer `cn.bing.com` and Sogou when domestic engines block.
- Configure `[search] provider = "metaso"` in config.toml for a more reliable default `web_search` backend.

## License

MIT
