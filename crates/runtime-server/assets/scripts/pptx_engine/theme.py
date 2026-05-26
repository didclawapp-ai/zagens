"""pptx_engine/theme.py — theme resolution and presets.

Migrated from write_pptx.py. Provides resolve_theme() and THEMES.
"""

import re

try:
    from pptx.dml.color import RGBColor
except ImportError:
    RGBColor = None  # deferred error in resolve_theme

_HEX_RE = re.compile(r"^#?([0-9a-fA-F]{6})$")


def _hex_to_rgb(s):
    m = _HEX_RE.match(str(s).strip())
    if not m:
        raise ValueError(f"invalid hex color: {s!r}")
    raw = int(m.group(1), 16)
    return RGBColor((raw >> 16) & 0xFF, (raw >> 8) & 0xFF, raw & 0xFF)


THEMES = {
    "dark": {
        "bg":     RGBColor(0x1A, 0x1A, 0x2E),
        "accent": RGBColor(0x00, 0xD4, 0xAA),
        "title":  RGBColor(0xFF, 0xFF, 0xFF),
        "body":   RGBColor(0xE0, 0xE4, 0xEC),
        "muted":  RGBColor(0x88, 0x90, 0xA0),
        "font":   "Segoe UI",
        "table_header_bg":  RGBColor(0x00, 0xD4, 0xAA),
        "table_header_fg":  RGBColor(0xFF, 0xFF, 0xFF),
        "table_row_bg":     RGBColor(0x1A, 0x1A, 0x2E),
        "table_alt_bg":     RGBColor(0x24, 0x24, 0x3A),
        "table_border":     RGBColor(0x00, 0xD4, 0xAA),
    },
    "light": {
        "bg":     RGBColor(0xFF, 0xFF, 0xFF),
        "accent": RGBColor(0x25, 0x63, 0xEB),
        "title":  RGBColor(0x11, 0x1F, 0x3D),
        "body":   RGBColor(0x37, 0x40, 0x51),
        "muted":  RGBColor(0x6B, 0x72, 0x80),
        "font":   "Calibri",
        "table_header_bg":  RGBColor(0x25, 0x63, 0xEB),
        "table_header_fg":  RGBColor(0xFF, 0xFF, 0xFF),
        "table_row_bg":     RGBColor(0xFF, 0xFF, 0xFF),
        "table_alt_bg":     RGBColor(0xF3, 0xF4, 0xF6),
        "table_border":     RGBColor(0x25, 0x63, 0xEB),
    },
    "warm": {
        "bg":     RGBColor(0xFF, 0xF8, 0xF0),
        "accent": RGBColor(0xE0, 0x7B, 0x3C),
        "title":  RGBColor(0x3D, 0x28, 0x1F),
        "body":   RGBColor(0x5C, 0x4A, 0x3E),
        "muted":  RGBColor(0x9C, 0x8B, 0x7E),
        "font":   "Georgia",
        "table_header_bg":  RGBColor(0xE0, 0x7B, 0x3C),
        "table_header_fg":  RGBColor(0xFF, 0xFF, 0xFF),
        "table_row_bg":     RGBColor(0xFF, 0xF8, 0xF0),
        "table_alt_bg":     RGBColor(0xFF, 0xF0, 0xE0),
        "table_border":     RGBColor(0xE0, 0x7B, 0x3C),
    },
    "minimal": {
        "bg":     RGBColor(0xFA, 0xFA, 0xFA),
        "accent": RGBColor(0x1A, 0x1A, 0x1A),
        "title":  RGBColor(0x0A, 0x0A, 0x0A),
        "body":   RGBColor(0x3D, 0x3D, 0x3D),
        "muted":  RGBColor(0x9E, 0x9E, 0x9E),
        "font":   "Helvetica",
        "table_header_bg":  RGBColor(0x1A, 0x1A, 0x1A),
        "table_header_fg":  RGBColor(0xFF, 0xFF, 0xFF),
        "table_row_bg":     RGBColor(0xFA, 0xFA, 0xFA),
        "table_alt_bg":     RGBColor(0xEE, 0xEE, 0xEE),
        "table_border":     RGBColor(0x1A, 0x1A, 0x1A),
    },
}

_THEME_KEYS = ("bg", "accent", "title", "body", "muted", "font")


def resolve_theme(raw, fallback=None):
    """Resolve theme from string name or custom dict.

    Args:
        raw: None, a preset name string, or a dict with color keys.
        fallback: theme to use when raw is None.

    Returns:
        A theme dict with RGBColor values.

    Note:
        When raw is a preset string, fallback is NOT used — the preset
        returns its own full dict. Custom dicts inherit table colors from
        the 'dark' preset.
    """
    if raw is None:
        raw = fallback
    if raw is None:
        raw = "dark"

    if isinstance(raw, str):
        return THEMES.get(raw, THEMES["dark"])

    if isinstance(raw, dict):
        for k in _THEME_KEYS:
            if k not in raw:
                raise ValueError(f"custom theme missing key: {k}")
        # Shallow copy — RGBColor values are immutable, so this is safe
        # and prevents custom themes from mutating THEMES["dark"] in place.
        base = dict(THEMES["dark"])
        base["bg"]     = _hex_to_rgb(str(raw["bg"]))
        base["accent"] = _hex_to_rgb(str(raw["accent"]))
        base["title"]  = _hex_to_rgb(str(raw["title"]))
        base["body"]   = _hex_to_rgb(str(raw["body"]))
        base["muted"]  = _hex_to_rgb(str(raw["muted"]))
        base["font"]   = str(raw["font"])
        return base

    raise ValueError(f"theme must be a string or dict, got {type(raw).__name__}")
