"""pptx_engine/charts.py — native OOXML chart helpers.

Migrated from write_pptx.py. Provides CHART_TYPES and _add_chart().
Phase 1 will add multi-chart-per-slide support and combo (bar+line) strategy.
"""

try:
    from pptx.chart.data import CategoryChartData, XyChartData
    from pptx.enum.chart import XL_CHART_TYPE
    from pptx.util import Pt
except ImportError:
    CategoryChartData = None
    XyChartData = None
    XL_CHART_TYPE = None

CHART_TYPES = {
    "bar":            XL_CHART_TYPE.COLUMN_CLUSTERED,
    "line":           XL_CHART_TYPE.LINE_MARKERS,
    "pie":            XL_CHART_TYPE.PIE,
    "stacked_bar":    XL_CHART_TYPE.COLUMN_STACKED,
    "stacked_bar_pct": XL_CHART_TYPE.COLUMN_STACKED_100,
    "area":           XL_CHART_TYPE.AREA,
    "scatter":        XL_CHART_TYPE.XY_SCATTER,
    "donut":          XL_CHART_TYPE.DOUGHNUT,
    "combo":          None,  # handled specially — bar + line overlay
}


def add_chart(slide, chart_data, left, top, width, height, t=None):
    """Add a styled OOXML chart at the given position.

    Args:
        slide: pptx slide object.
        chart_data: dict with type, categories, series, and optional
                    chart_title, x_label, y_label, data_labels.
        left, top, width, height: Inches values (EMU) for positioning.
        t: theme dict (optional). Body color applied to all chart text
           so labels remain readable on dark backgrounds.

    Returns:
        Height used (inches), or 0 if chart could not be created.
    """
    cd = chart_data
    chart_type = cd.get("type", "bar")
    is_combo = (chart_type == "combo")
    ct = XL_CHART_TYPE.COLUMN_CLUSTERED if is_combo else CHART_TYPES.get(
        chart_type, XL_CHART_TYPE.COLUMN_CLUSTERED
    )
    categories = cd.get("categories", [])
    series_list = cd.get("series", [])

    if not categories or not series_list:
        return 0

    is_scatter = (chart_type == "scatter")
    if is_scatter:
        chart_data_obj = XyChartData()
        x_vals = [float(c) for c in categories]
        for s in series_list:
            y_vals = [float(v) for v in s.get("values", [])]
            xy_series = chart_data_obj.add_series(str(s.get("name", "")))
            for x, y in zip(x_vals, y_vals):
                xy_series.add_data_point(x, y)
    else:
        chart_data_obj = CategoryChartData()
        chart_data_obj.categories = [str(c) for c in categories]
        for s in series_list:
            vals = [float(v) for v in s.get("values", [])]
            chart_data_obj.add_series(str(s.get("name", "")), vals)

    chart_frame = slide.shapes.add_chart(ct, left, top, width, height, chart_data_obj)
    chart = chart_frame.chart
    chart.has_legend = len(series_list) > 1 or is_combo

    # ── Combo: convert last series to line ──
    if is_combo and len(series_list) >= 2:
        # Last series → line on secondary axis
        line_series = chart.series[-1]
        line_series.chart_type = XL_CHART_TYPE.LINE_MARKERS

    chart_title_text = cd.get("chart_title", "")
    if chart_title_text:
        chart.has_title = True
        chart.chart_title.text_frame.paragraphs[0].text = chart_title_text
        chart.chart_title.text_frame.paragraphs[0].font.size = Pt(14)

    x_label = cd.get("x_label", "")
    y_label = cd.get("y_label", "")
    if x_label and chart.category_axis:
        chart.category_axis.has_title = True
        chart.category_axis.axis_title.text_frame.paragraphs[0].text = x_label
    if y_label and chart.value_axis:
        chart.value_axis.has_title = True
        chart.value_axis.axis_title.text_frame.paragraphs[0].text = y_label

    # Data labels — default ON; set "data_labels": false to suppress
    if cd.get("data_labels") is not False and chart.series:
        try:
            for s in chart.series:
                s.has_data_labels = True
                s.data_labels.font.size = Pt(9)
        except AttributeError:
            pass  # scatter (XySeries) doesn't support data_labels the same way

    # ── Apply theme colors so text is readable on dark backgrounds ──
    if t:
        body_color = t["body"]
        muted_color = t.get("muted", body_color)

        # Chart title
        if chart.has_title:
            chart.chart_title.text_frame.paragraphs[0].font.color.rgb = body_color

        # Axis titles / tick labels — Pie/Donut have no axes
        try:
            if x_label and chart.category_axis and chart.category_axis.has_title:
                chart.category_axis.axis_title.text_frame.paragraphs[0].font.color.rgb = muted_color
            if chart.category_axis:
                chart.category_axis.tick_labels.font.color.rgb = muted_color
        except (AttributeError, ValueError, TypeError):
            pass
        try:
            if y_label and chart.value_axis and chart.value_axis.has_title:
                chart.value_axis.axis_title.text_frame.paragraphs[0].font.color.rgb = muted_color
            if chart.value_axis:
                chart.value_axis.tick_labels.font.color.rgb = muted_color
        except (AttributeError, ValueError, TypeError):
            pass

        # Legend
        if chart.has_legend:
            chart.legend.font.color.rgb = muted_color

        # Data label colors
        if cd.get("data_labels") is not False and chart.series:
            try:
                for s in chart.series:
                    s.data_labels.font.color.rgb = body_color
            except AttributeError:
                pass

        # Chart area background → transparent (blend with slide bg)
        try:
            chart.chart_style = None  # remove default white style
        except Exception:
            pass
        try:
            chart.fill.background()
        except Exception:
            pass

    return _inches(height)  # EMU → inches


def _inches(emu_val):
    return emu_val / 914400.0
