#!/usr/bin/env python3
"""write_pdf.py — generate .pdf from JSON stdin payload (ReportLab).

Called by WriteOfficeTool (bundled PBS Python or ~/.deepseek/office-py venv).
Args: --output PATH   (output .pdf path)
Stdin: JSON with `title`, `blocks`, optional `page`, `font`, `header`, `footer`.

Blocks (same vocabulary as write_docx.py where possible):
  { "type": "heading", "level": 1..6, "text": "..." }
  { "type": "paragraph", "text": "..." | "runs": [{ text, bold?, italic?, color?, size? }], "align"?: left|center|right|justify }
  { "type": "list", "style": "bullet"|"number", "items": [str | {text, subitems?}] }
  { "type": "table", "headers": [str], "rows": [[value...]] }
  { "type": "image", "path": "file.png", "width"?: px, "height"?: px, "caption"?: str }
  { "type": "page_break" }
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from xml.sax.saxutils import escape

try:
    from reportlab.lib import colors
    from reportlab.lib.enums import TA_CENTER, TA_JUSTIFY, TA_LEFT, TA_RIGHT
    from reportlab.lib.pagesizes import A3, A4, letter
    from reportlab.lib.styles import ParagraphStyle, getSampleStyleSheet
    from reportlab.lib.units import cm
    from reportlab.pdfbase import pdfmetrics
    from reportlab.pdfbase.ttfonts import TTFont
    from reportlab.platypus import (
        Image,
        PageBreak,
        Paragraph,
        SimpleDocTemplate,
        Spacer,
        Table,
        TableStyle,
    )
except ImportError as e:
    print(f"ERROR: {e}", file=sys.stderr)
    print("请运行: pip install reportlab", file=sys.stderr)
    sys.exit(1)

PAPER_SIZES = {
    "A4": A4,
    "A3": A3,
    "Letter": letter,
}

ALIGN_MAP = {
    "left": TA_LEFT,
    "center": TA_CENTER,
    "right": TA_RIGHT,
    "justify": TA_JUSTIFY,
}

# Platform font candidates for CJK (first match wins).
FONT_CANDIDATES: list[tuple[str, str]] = [
    ("MSYaHei", r"C:\Windows\Fonts\msyh.ttc"),
    ("MSYaHei", r"C:\Windows\Fonts\msyhbd.ttc"),
    ("SimSun", r"C:\Windows\Fonts\simsun.ttc"),
    ("PingFang", "/System/Library/Fonts/PingFang.ttc"),
    ("PingFang", "/System/Library/Fonts/Supplemental/PingFang.ttc"),
    ("STHeiti", "/System/Library/Fonts/STHeiti Light.ttc"),
    ("NotoSansCJK", "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc"),
    ("NotoSansCJK", "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc"),
    ("NotoSansCJK", "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc"),
]


def register_body_font() -> str:
    """Register a CJK-capable font when available."""
    for name, path in FONT_CANDIDATES:
        if not os.path.isfile(path):
            continue
        try:
            pdfmetrics.registerFont(TTFont(name, path))
            return name
        except Exception:
            continue
    return "Helvetica"


def runs_to_markup(runs: list, font_name: str) -> str:
    parts: list[str] = []
    for r in runs:
        text = escape(str(r.get("text", "")))
        if not text:
            continue
        open_tags = ""
        close_tags = ""
        if r.get("bold"):
            open_tags += "<b>"
            close_tags = "</b>" + close_tags
        if r.get("italic"):
            open_tags += "<i>"
            close_tags = "</i>" + close_tags
        color = r.get("color", "")
        size = r.get("size")
        font_attrs = f'name="{font_name}"'
        if color and color.startswith("#") and len(color) == 7:
            font_attrs += f' color="{color}"'
        if size is not None:
            font_attrs += f' size="{float(size)}"'
        parts.append(f"<font {font_attrs}>{open_tags}{text}{close_tags}</font>")
    return "".join(parts)


def build_styles(font_name: str, base_size: float = 11.0):
    styles = getSampleStyleSheet()
    body = ParagraphStyle(
        "Body",
        parent=styles["Normal"],
        fontName=font_name,
        fontSize=base_size,
        leading=base_size * 1.35,
        spaceAfter=6,
    )
    headings = {}
    for level in range(1, 7):
        headings[level] = ParagraphStyle(
            f"H{level}",
            parent=styles["Heading1"],
            fontName=font_name,
            fontSize=max(22 - level * 2, 11),
            leading=max(24 - level * 2, 13),
            spaceBefore=10,
            spaceAfter=6,
            textColor=colors.HexColor("#1a1a2e"),
        )
    title_style = ParagraphStyle(
        "DocTitle",
        parent=styles["Title"],
        fontName=font_name,
        fontSize=18,
        leading=22,
        alignment=TA_CENTER,
        spaceAfter=14,
    )
    caption_style = ParagraphStyle(
        "Caption",
        parent=body,
        fontName=font_name,
        fontSize=base_size - 1,
        alignment=TA_CENTER,
        textColor=colors.grey,
    )
    return body, headings, title_style, caption_style


def resolve_page_size(page_cfg: dict | None):
    paper = (page_cfg or {}).get("paper", "A4")
    size = PAPER_SIZES.get(paper, A4)
    if (page_cfg or {}).get("orientation") == "landscape":
        size = (size[1], size[0])
    return size


def resolve_margins(page_cfg: dict | None) -> tuple[float, float, float, float]:
    m = (page_cfg or {}).get("margins") or {}
    return (
        float(m.get("left", 3.18)) * cm,
        float(m.get("right", 3.18)) * cm,
        float(m.get("top", 2.54)) * cm,
        float(m.get("bottom", 2.54)) * cm,
    )


def add_list_story(story: list, block: dict, body_style: ParagraphStyle, font_name: str):
    style = block.get("style", "bullet")
    items = block.get("items", [])
    for i, item in enumerate(items):
        if isinstance(item, dict):
            text = str(item.get("text", ""))
            subitems = item.get("subitems") or []
        else:
            text = str(item)
            subitems = []
        prefix = f"{i + 1}. " if style == "number" else "• "
        story.append(Paragraph(f"{prefix}{escape(text)}", body_style))
        for si in subitems:
            si_text = si.get("text", si) if isinstance(si, dict) else str(si)
            story.append(
                Paragraph(
                    f"{'  '}{prefix}{escape(str(si_text))}",
                    body_style,
                )
            )


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--output", required=True, help="Output .pdf path")
    args = parser.parse_args()

    payload = json.load(sys.stdin)
    font_name = register_body_font()
    font_cfg = payload.get("font") or {}
    base_size = float(font_cfg.get("size", 11))

    body_style, heading_styles, title_style, caption_style = build_styles(
        font_name, base_size
    )

    page_cfg = payload.get("page")
    pagesize = resolve_page_size(page_cfg)
    left, right, top, bottom = resolve_margins(page_cfg)

    header_cfg = payload.get("header") or {}
    footer_cfg = payload.get("footer") or {}

    def header_text() -> str:
        if header_cfg.get("text"):
            return str(header_cfg["text"])
        parts = [
            header_cfg.get("left", ""),
            header_cfg.get("center", ""),
            header_cfg.get("right", ""),
        ]
        return "  ".join(p for p in parts if p)

    def footer_text() -> str:
        if footer_cfg.get("text"):
            return str(footer_cfg["text"])
        parts = [
            footer_cfg.get("left", ""),
            footer_cfg.get("center", ""),
            footer_cfg.get("right", ""),
        ]
        return "  ".join(p for p in parts if p)

    htxt = header_text()
    ftxt = footer_text()

    def on_page(canvas, doc):
        canvas.saveState()
        canvas.setFont(font_name, 9)
        w, h = doc.pagesize
        if htxt:
            canvas.drawCentredString(w / 2, h - 1.2 * cm, htxt)
        if ftxt:
            canvas.drawCentredString(w / 2, 0.8 * cm, ftxt)
        canvas.drawRightString(w - right, 0.8 * cm, f"第 {doc.page} 页")
        canvas.restoreState()

    doc = SimpleDocTemplate(
        args.output,
        pagesize=pagesize,
        leftMargin=left,
        rightMargin=right,
        topMargin=top,
        bottomMargin=bottom,
        title=payload.get("title", ""),
    )

    story: list = []

    title = payload.get("title", "")
    if title:
        story.append(Paragraph(escape(title), title_style))
        story.append(Spacer(1, 0.3 * cm))

    output_dir = os.path.dirname(os.path.abspath(args.output))

    for block in payload.get("blocks", []):
        typ = block.get("type", "paragraph")

        if typ == "heading":
            level = max(1, min(int(block.get("level", 1)), 6))
            text = escape(block.get("text", ""))
            story.append(Paragraph(text, heading_styles[level]))

        elif typ == "paragraph":
            if block.get("page_break_before"):
                story.append(PageBreak())
            runs = block.get("runs")
            if runs and isinstance(runs, list):
                markup = runs_to_markup(runs, font_name)
                p_style = ParagraphStyle(
                    "RunPara",
                    parent=body_style,
                    alignment=ALIGN_MAP.get(block.get("align", "left"), TA_LEFT),
                )
                story.append(Paragraph(markup or " ", p_style))
            else:
                p_style = ParagraphStyle(
                    "PlainPara",
                    parent=body_style,
                    alignment=ALIGN_MAP.get(block.get("align", "left"), TA_LEFT),
                )
                story.append(Paragraph(escape(block.get("text", "")), p_style))

        elif typ == "list":
            add_list_story(story, block, body_style, font_name)

        elif typ == "table":
            headers = block.get("headers") or []
            rows = block.get("rows") or []
            if not headers and not rows:
                continue
            ncols = len(headers) if headers else (len(rows[0]) if rows else 1)
            data: list[list[str]] = []
            if headers:
                data.append([str(h) for h in headers])
            for row in rows:
                data.append([str(v) if v is not None else "" for v in row])
            col_width = (pagesize[0] - left - right) / max(ncols, 1)
            table = Table(data, colWidths=[col_width] * ncols)
            style_cmds = [
                ("FONTNAME", (0, 0), (-1, -1), font_name),
                ("FONTSIZE", (0, 0), (-1, -1), base_size),
                ("ALIGN", (0, 0), (-1, -1), "LEFT"),
                ("VALIGN", (0, 0), (-1, -1), "MIDDLE"),
                ("GRID", (0, 0), (-1, -1), 0.5, colors.grey),
            ]
            if headers:
                style_cmds.extend(
                    [
                        ("BACKGROUND", (0, 0), (-1, 0), colors.HexColor("#4472C4")),
                        ("TEXTCOLOR", (0, 0), (-1, 0), colors.white),
                        ("ROWBACKGROUNDS", (0, 1), (-1, -1), [colors.white, colors.HexColor("#F2F2F2")]),
                    ]
                )
            else:
                style_cmds.append(
                    ("ROWBACKGROUNDS", (0, 0), (-1, -1), [colors.white, colors.HexColor("#F2F2F2")])
                )
            table.setStyle(TableStyle(style_cmds))
            story.append(Spacer(1, 0.2 * cm))
            story.append(table)
            story.append(Spacer(1, 0.4 * cm))

        elif typ == "image":
            img_path = block.get("path", "")
            if not img_path:
                continue
            if not os.path.isabs(img_path):
                img_path = os.path.join(output_dir, img_path)
            if os.path.isfile(img_path):
                w_px = block.get("width")
                h_px = block.get("height")
                kw: dict = {}
                if w_px:
                    kw["width"] = float(w_px) * 72.0 / 96.0
                if h_px:
                    kw["height"] = float(h_px) * 72.0 / 96.0
                img = Image(img_path, **kw)
                story.append(img)
                caption = block.get("caption", "")
                if caption:
                    story.append(Paragraph(escape(caption), caption_style))
                story.append(Spacer(1, 0.3 * cm))

        elif typ == "page_break":
            story.append(PageBreak())

    doc.build(story, onFirstPage=on_page, onLaterPages=on_page)
    print("OK")


if __name__ == "__main__":
    main()
