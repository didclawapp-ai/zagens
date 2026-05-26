"""pptx_engine/mpl.py — declarative matplotlib chart renderer.

Phase 2: renders gantt, fishbone, radar, funnel charts to PNG via matplotlib.
Only accepts declarative JSON — no exec() of LLM-provided code (§7.1).

Dependencies: matplotlib (optional — clear error if missing), tempfile.
"""

import os
import sys
import tempfile
import warnings


def _check_matplotlib():
    """Import matplotlib; return (plt, mdates, patches) or (None, None, None)."""
    try:
        import matplotlib
        matplotlib.use("Agg")
        import matplotlib.pyplot as plt
        import matplotlib.dates as mdates
        from matplotlib.patches import FancyBboxPatch
        return plt, mdates, FancyBboxPatch
    except ImportError:
        return None, None, None


# ── Font fallback (CJK) ──────────────────────────────────────────────────

def _detect_cjk_font():
    """Return the first available CJK font name, or None."""
    try:
        import matplotlib.font_manager as fm
        available = {f.name for f in fm.fontManager.ttflist}
        candidates = [
            "Microsoft YaHei",
            "PingFang SC",
            "Heiti SC",
            "Noto Sans CJK SC",
            "SimHei",
        ]
        for name in candidates:
            if name in available:
                return name
    except Exception:
        pass
    return None


def _resolve_font(user_font):
    """Resolve font: user-specified → auto-detect → None (English fallback)."""
    if user_font:
        try:
            import matplotlib.font_manager as fm
            available = {f.name for f in fm.fontManager.ttflist}
            if user_font in available:
                return user_font
        except Exception:
            pass
    return _detect_cjk_font()


# ── Render dispatcher ────────────────────────────────────────────────────

def render_mpl_chart(mpl_data, output_dir=None):
    """Render a declarative matplotlib chart to PNG.

    Args:
        mpl_data: dict with 'chart_type', 'data', optional 'title', 'width',
                  'height', 'dpi', 'font', 'color_scheme'.
        output_dir: directory for output PNG (default: system temp).

    Returns:
        Path to generated PNG file, or None on failure.
    """
    plt_mod, mdates, FancyBboxPatch_mod = _check_matplotlib()
    if plt_mod is None:
        print("ERROR: matplotlib not installed — cannot render mpl chart", file=sys.stderr)
        return None

    chart_type = mpl_data.get("chart_type", "bar")
    data = mpl_data.get("data", {})
    title = mpl_data.get("title", "")
    width = mpl_data.get("width", 1200)
    height = mpl_data.get("height", 600)
    dpi = mpl_data.get("dpi", 100)
    user_font = mpl_data.get("font")

    font = _resolve_font(user_font)
    if font:
        plt_mod.rcParams["font.family"] = font
    else:
        print("WARNING: no CJK font detected — chart labels may show tofu", file=sys.stderr)
        plt_mod.rcParams["font.family"] = "sans-serif"

    plt_mod.rcParams["font.size"] = 10

    figsize = (width / dpi, height / dpi)

    try:
        if chart_type == "gantt":
            png_path = _render_gantt(plt_mod, mdates, data, title, figsize, dpi, output_dir)
        elif chart_type == "fishbone":
            png_path = _render_fishbone(plt_mod, data, title, figsize, dpi, output_dir)
        elif chart_type == "radar":
            png_path = _render_radar(plt_mod, data, title, figsize, dpi, output_dir)
        elif chart_type == "funnel":
            png_path = _render_funnel(plt_mod, data, title, figsize, dpi, output_dir)
        else:
            print(f"WARNING: unknown mpl chart_type: {chart_type!r}", file=sys.stderr)
            return None

        return png_path
    except Exception as e:
        print(f"ERROR: mpl chart '{chart_type}' failed: {e}", file=sys.stderr)
        return None


# ── Gantt chart ──────────────────────────────────────────────────────────

def _render_gantt(plt, mdates, data, title, figsize, dpi, output_dir):
    """Render a Gantt chart.

    data.tasks: [{name, start, end, owner?, status?}]
    start/end: 'YYYY-MM-DD' strings.
    status: 'done'|'active'|'pending' → color.
    """
    tasks = data.get("tasks", [])
    if not tasks:
        return None

    import datetime as dt

    colors = {"done": "#4CAF50", "active": "#2196F3", "pending": "#9E9E9E"}

    fig, ax = plt.subplots(figsize=figsize, dpi=dpi)
    ax.set_title(title, fontsize=14, fontweight="bold", pad=15)

    n = len(tasks)
    y_ticks = list(range(n))
    labels = []
    min_date = None
    max_date = None

    for i, task in enumerate(reversed(tasks)):  # top-down
        name = task.get("name", f"Task {i}")
        owner = task.get("owner", "")
        label = f"{name}" + (f" ({owner})" if owner else "")
        labels.append(label)

        start_str = task.get("start", "")
        end_str = task.get("end", "")
        try:
            start = dt.datetime.strptime(start_str, "%Y-%m-%d")
            end = dt.datetime.strptime(end_str, "%Y-%m-%d")
        except ValueError:
            continue

        if min_date is None or start < min_date:
            min_date = start
        if max_date is None or end > max_date:
            max_date = end

        duration = (end - start).days
        status = task.get("status", "pending")
        color = colors.get(status, "#9E9E9E")

        ax.barh(i, duration, left=start, height=0.5, color=color, edgecolor="white",
                linewidth=0.5, align="center")

    ax.set_yticks(y_ticks)
    ax.set_yticklabels(labels, fontsize=9)
    ax.invert_yaxis()

    if min_date and max_date:
        pad = (max_date - min_date) * 0.05
        ax.set_xlim(min_date - pad, max_date + pad)
        ax.xaxis.set_major_formatter(mdates.DateFormatter("%m-%d"))
        ax.xaxis.set_major_locator(mdates.WeekdayLocator(interval=1))
        fig.autofmt_xdate(rotation=30, ha="right")

    ax.set_xlabel("日期", fontsize=10)
    ax.grid(axis="x", alpha=0.3)
    ax.spines["top"].set_visible(False)
    ax.spines["right"].set_visible(False)

    # Legend
    from matplotlib.patches import Patch
    legend_elements = [
        Patch(facecolor=colors["done"], label="已完成"),
        Patch(facecolor=colors["active"], label="进行中"),
        Patch(facecolor=colors["pending"], label="待开始"),
    ]
    ax.legend(handles=legend_elements, loc="lower right", fontsize=8, framealpha=0.8)

    plt.tight_layout()
    path = _save_png(fig, output_dir, "gantt")
    plt.close(fig)
    return path


# ── Fishbone (Ishikawa) chart ────────────────────────────────────────────

def _render_fishbone(plt, data, title, figsize, dpi, output_dir):
    """Render a fishbone / Ishikawa diagram.

    data.categories: [{name, causes: [str]}]
    data.problem: str (head label)
    """
    categories = data.get("categories", [])
    problem = data.get("problem", "Problem")
    if not categories:
        return None

    fig, ax = plt.subplots(figsize=figsize, dpi=dpi)
    ax.set_title(title or "鱼骨图分析", fontsize=14, fontweight="bold", pad=15)
    ax.set_xlim(-10, 12)
    ax.set_ylim(-10, 10)
    ax.axis("off")

    # Spine
    ax.annotate("", xy=(8, 0), xytext=(-8, 0),
                arrowprops=dict(arrowstyle="->", color="black", lw=3))

    # Head (problem statement)
    ax.text(8.5, 0, problem, fontsize=12, fontweight="bold", va="center",
            bbox=dict(boxstyle="round,pad=0.3", facecolor="#FFE0E0", edgecolor="#CC0000"))

    n = len(categories)
    for i, cat in enumerate(categories):
        # Compute angle for the rib
        angle = (i + 1) * 180 / (n + 1)
        if angle > 90:
            angle = 180 - angle
            side = -1
        else:
            side = 1

        rad = angle * 3.14159 / 180
        rib_x = 6 * (i - n / 2) / n
        rib_y = 7 if i % 2 == 0 else -7

        # Rib line
        ax.plot([0, rib_x], [0, rib_y], color="#555555", lw=1.5)

        # Category label
        ax.text(rib_x, rib_y + (0.8 if rib_y > 0 else -0.8),
                cat.get("name", f"Cat {i+1}"), fontsize=11, fontweight="bold",
                ha="center", va="center",
                bbox=dict(boxstyle="round,pad=0.2", facecolor="#E8F0FE", edgecolor="#3366CC"))

        # Causes (small branches)
        causes = cat.get("causes", [])
        for j, cause in enumerate(causes):
            offset = (j - len(causes) / 2 + 0.5) * 1.2
            cx = rib_x * 0.6 + offset * 0.3
            cy = rib_y * 0.6 + offset
            ax.plot([rib_x * 0.9, cx], [rib_y * 0.9, cy], color="#999999", lw=0.8)
            ax.text(cx, cy, cause, fontsize=8, ha="center", va="center", alpha=0.8)

    plt.tight_layout()
    path = _save_png(fig, output_dir, "fishbone")
    plt.close(fig)
    return path


# ── Radar chart ──────────────────────────────────────────────────────────

def _render_radar(plt, data, title, figsize, dpi, output_dir):
    """Render a radar/spider chart.

    data.categories: [str] (axis labels)
    data.series: [{name, values: [float]}]
    """
    import numpy as np

    categories = data.get("categories", [])
    series_list = data.get("series", [])
    if not categories or not series_list:
        return None

    n = len(categories)
    angles = np.linspace(0, 2 * np.pi, n, endpoint=False).tolist()
    angles += angles[:1]  # close the loop

    fig, ax = plt.subplots(figsize=figsize, dpi=dpi, subplot_kw=dict(polar=True))
    ax.set_title(title, fontsize=14, fontweight="bold", pad=20)

    colors = plt.cm.tab10.colors
    for si, series in enumerate(series_list):
        values = series.get("values", [])
        if len(values) != n:
            continue
        values_closed = values + values[:1]
        color = colors[si % len(colors)]
        ax.fill(angles, values_closed, alpha=0.15, color=color)
        ax.plot(angles, values_closed, "o-", linewidth=2, color=color,
                label=series.get("name", f"Series {si+1}"), markersize=4)

    ax.set_xticks(angles[:-1])
    ax.set_xticklabels(categories, fontsize=9)
    ax.set_yticklabels([])
    ax.legend(loc="upper right", bbox_to_anchor=(1.3, 1.0), fontsize=8)

    plt.tight_layout()
    path = _save_png(fig, output_dir, "radar")
    plt.close(fig)
    return path


# ── Funnel chart ─────────────────────────────────────────────────────────

def _render_funnel(plt, data, title, figsize, dpi, output_dir):
    """Render a funnel chart.

    data.stages: [{name, value}]
    """
    stages = data.get("stages", [])
    if not stages:
        return None

    names = [s.get("name", "") for s in stages]
    values = [float(s.get("value", 0)) for s in stages]
    total = max(values) if values else 1

    fig, ax = plt.subplots(figsize=figsize, dpi=dpi)
    ax.set_title(title, fontsize=14, fontweight="bold", pad=15)
    ax.axis("off")

    n = len(stages)
    colors = plt.cm.Blues([0.3 + 0.6 * i / (n - 1) for i in range(n)]) if n > 1 else [plt.cm.Blues(0.6)]

    bar_height = 1.2
    total_height = n * (bar_height + 0.4)
    y_top = total_height / 2

    for i, (name, val) in enumerate(zip(names, values)):
        ratio = val / total if total > 0 else 1
        width = 6 * ratio
        y = y_top - i * (bar_height + 0.4)

        # Bar
        ax.barh(y, width, height=bar_height, color=colors[i], edgecolor="white", linewidth=1.5,
                left=(6 - width) / 2, align="center")

        # Label
        pct = f"{val / sum(values) * 100:.1f}%" if sum(values) > 0 else ""
        ax.text(3, y, f"{name}  {val}  {pct}", ha="center", va="center",
                fontsize=10, fontweight="bold", color="white")

    ax.set_xlim(0, 6)
    ax.set_ylim(-bar_height, total_height + bar_height)

    plt.tight_layout()
    path = _save_png(fig, output_dir, "funnel")
    plt.close(fig)
    return path


# ── Output helper ────────────────────────────────────────────────────────

def _save_png(fig, output_dir, prefix):
    """Save figure to PNG and return path."""
    if output_dir is None:
        output_dir = tempfile.gettempdir()
    os.makedirs(output_dir, exist_ok=True)
    fd, path = tempfile.mkstemp(suffix=".png", prefix=f"pptx_{prefix}_", dir=output_dir)
    os.close(fd)
    fig.savefig(path, dpi=fig.dpi, bbox_inches="tight", pad_inches=0.2,
                facecolor="white", edgecolor="none")
    return path
