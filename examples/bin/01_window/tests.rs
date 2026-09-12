use super::*;

#[test]
fn paint_renderer_mode_parser_honors_default_and_fail_closed() {
    assert_eq!(
        parse_paint_renderer_mode(Some(RETAINED_AUTO_LABEL), ViewportPaintRendererMode::Legacy),
        ViewportPaintRendererMode::RetainedAuto
    );
    assert_eq!(
        parse_paint_renderer_mode(None, ViewportPaintRendererMode::RetainedAuto),
        ViewportPaintRendererMode::RetainedAuto
    );
    assert_eq!(
        parse_paint_renderer_mode(None, ViewportPaintRendererMode::Legacy),
        ViewportPaintRendererMode::Legacy
    );
    for value in [
        Some(LEGACY_LABEL),
        Some(""),
        Some("artifact"),
        Some("RETAINED-AUTO"),
    ] {
        assert_eq!(
            parse_paint_renderer_mode(value, ViewportPaintRendererMode::RetainedAuto),
            ViewportPaintRendererMode::Legacy
        );
    }
}

#[test]
fn query_parser_reads_only_the_exact_pilot_key() {
    assert_eq!(
        query_parameter(
            "?unrelated=1&rfgui-paint=retained-auto&after=2",
            PAINT_RENDERER_QUERY,
        ),
        Some(RETAINED_AUTO_LABEL)
    );
    assert_eq!(
        query_parameter("?rfgui-paint=legacy", PAINT_RENDERER_QUERY),
        Some(LEGACY_LABEL)
    );
    assert_eq!(
        query_parameter("?rfgui-painter=retained-auto", PAINT_RENDERER_QUERY),
        None
    );
    assert_eq!(query_parameter("?rfgui-paint", PAINT_RENDERER_QUERY), None);
    assert_eq!(
        query_parameter(
            "?rfgui-paint=retained-auto&rfgui-paint=legacy",
            PAINT_RENDERER_QUERY,
        ),
        None
    );
    assert_eq!(
        query_parameter(
            "?rfgui-paint=legacy&rfgui-paint=retained-auto",
            PAINT_RENDERER_QUERY,
        ),
        None
    );
    assert_eq!(
        query_parameter(
            "?rfgui-paint=retained-auto&rfgui-paint=retained-auto",
            PAINT_RENDERER_QUERY,
        ),
        None
    );
}
