use super::*;

#[test]
fn cache_key_is_stable_for_identical_input() {
    let input = cache_fixture_input();
    let same_input = cache_fixture_input();
    let key = input.cache_key();
    let same_key = same_input.cache_key();
    let ifc = InlineFormattingContext::build(input.clone());

    assert_eq!(key, same_key);
    assert_eq!(ifc.cache_key(), &key);
    assert_eq!(
        cache_invalidation(&input, &same_input),
        InlineIfcInvalidation::Reuse
    );
}

#[test]
fn cache_evicts_coldest_entries_beyond_capacity() {
    let mut cache = InlineIfcCache::new();
    for width in [100.0_f32, 120.0, 140.0, 160.0, 180.0, 200.0] {
        let update = cache.update_with_options(
            plain_text_input("evict me"),
            InlineIfcLayoutOptions::new(Some(width), true),
        );
        assert!(update.rebuilt, "distinct widths must reshape");
    }
    assert!(
        cache.len() <= INLINE_IFC_CACHE_MAX_ENTRIES,
        "cache must stay bounded, len={}",
        cache.len()
    );
    // The most recent shape must survive eviction.
    let latest_key = plain_text_input("evict me")
        .cache_key_with_layout_options(InlineIfcLayoutOptions::new(Some(200.0), true));
    assert!(
        !matches!(
            cache.lookup_key(&latest_key),
            InlineIfcCacheLookup::Miss { .. }
        ),
        "latest entry must be retained"
    );
}

#[test]
fn cache_drops_derived_data_from_cold_entries() {
    let mut cache = InlineIfcCache::new();
    let first_options = InlineIfcLayoutOptions::new(Some(100.0), true);
    let first_key =
        plain_text_input("cold derived data").cache_key_with_layout_options(first_options);
    let _ = cache.update_with_options(plain_text_input("cold derived data"), first_options);
    let first_shape_key = InlineIfcShapeCacheKey::from_cache_key(&first_key);
    let first = cache.entries.get(&first_shape_key).expect("first shape");
    let _ = first.context.glyph_items_ref();
    assert!(first.context.glyph_items_cache.get().is_some());

    let _ = cache.update_with_options(
        plain_text_input("cold derived data"),
        InlineIfcLayoutOptions::new(Some(200.0), true),
    );

    let first = cache
        .entries
        .get(&first_shape_key)
        .expect("retained cold shape");
    assert!(first.context.glyph_items_cache.get().is_none());
}

#[test]
fn layout_cache_key_distinguishes_alignment() {
    let input = plain_text_input("align key");
    let left =
        input.cache_key_with_layout_options(InlineIfcLayoutOptions::new(Some(100.0), true));
    let center = input.cache_key_with_layout_options(
        InlineIfcLayoutOptions::new(Some(100.0), true).with_align(InlineIfcAlignment::Center),
    );
    assert_ne!(left, center, "alignment must participate in the shape key");
}

#[test]
fn cache_invalidation_reshapes_when_text_content_changes() {
    let previous = cache_fixture_input();
    let mut next = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { text, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    text.push_str("again");

    assert_eq!(
        cache_invalidation(&previous, &next),
        InlineIfcInvalidation::Reshape
    );
}

#[test]
fn cache_invalidation_reshapes_when_text_shape_style_or_width_changes() {
    let previous = cache_fixture_input();

    let mut font_size_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut font_size_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    style.as_mut().unwrap().font_size = 18.0;
    assert_eq!(
        cache_invalidation(&previous, &font_size_changed),
        InlineIfcInvalidation::Reshape
    );

    let mut font_weight_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut font_weight_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    style.as_mut().unwrap().font_weight = 700;
    assert_eq!(
        cache_invalidation(&previous, &font_weight_changed),
        InlineIfcInvalidation::Reshape
    );

    let mut line_height_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut line_height_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    style.as_mut().unwrap().line_height = 1.6;
    assert_eq!(
        cache_invalidation(&previous, &line_height_changed),
        InlineIfcInvalidation::Reshape
    );

    let width_changed = cache_fixture_input().with_max_width(96.0);
    assert_eq!(
        cache_invalidation(&previous, &width_changed),
        InlineIfcInvalidation::Reshape
    );
}

#[test]
fn cache_invalidation_reshapes_when_atomic_layout_inputs_change() {
    let previous = cache_fixture_input();

    let mut measured_size_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut measured_size_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::AtomicInlineBox { measurement, .. } = &mut children[1] else {
        panic!("expected atomic box");
    };
    measurement.measured_size = InlineIfcSize::new(36.0, 12.0);
    assert_eq!(
        cache_invalidation(&previous, &measured_size_changed),
        InlineIfcInvalidation::Reshape
    );

    let mut constraints_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut constraints_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::AtomicInlineBox { measurement, .. } = &mut children[1] else {
        panic!("expected atomic box");
    };
    measurement.constraints.max_width = Some(128.0);
    assert_eq!(
        cache_invalidation(&previous, &constraints_changed),
        InlineIfcInvalidation::Reshape
    );
}

#[test]
fn cache_invalidation_treats_brush_change_as_repaint_only() {
    let previous = cache_fixture_input();
    let mut next = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    style.as_mut().unwrap().brush = [200, 10, 10, 255];

    assert_eq!(
        cache_invalidation(&previous, &next),
        InlineIfcInvalidation::RepaintOnly
    );
}

#[test]
fn cache_lookup_reuses_same_input() {
    let mut cache = InlineIfcCache::new();
    let input = cache_fixture_input();
    let expected_key = input.cache_key();

    let inserted = cache.put(input.clone());
    assert_eq!(inserted.cache_key(), &expected_key);
    assert_eq!(cache.len(), 1);

    let InlineIfcCacheLookup::Reuse(entry) = cache.lookup_input(&input) else {
        panic!("same input should reuse cached IFC entry");
    };
    assert_eq!(entry.cache_key(), &expected_key);
    assert_eq!(entry.context().backing_text(), "cache me ");
}

#[test]
fn cache_lookup_treats_brush_only_change_as_repaint_only() {
    let mut cache = InlineIfcCache::new();
    let previous = cache_fixture_input();
    let previous_key = previous.cache_key();
    cache.put(previous);

    let mut next = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    style.as_mut().unwrap().brush = [200, 10, 10, 255];
    let next_key = next.cache_key();

    assert_eq!(previous_key.content, next_key.content);
    assert_eq!(previous_key.layout, next_key.layout);
    assert_ne!(previous_key.paint, next_key.paint);

    let InlineIfcCacheLookup::RepaintOnly(entry) = cache.lookup_input(&next) else {
        panic!("brush-only change should keep shape cache reusable");
    };
    let entry_shape_key = InlineIfcShapeCacheKey::from_cache_key(entry.cache_key());
    assert_eq!(entry_shape_key.content, next_key.content);
    assert_eq!(entry_shape_key.layout, next_key.layout);
    assert_eq!(entry.cache_key(), &previous_key);
}

#[test]
fn cache_lookup_misses_when_shape_inputs_change() {
    let mut cache = InlineIfcCache::new();
    let previous = cache_fixture_input();
    cache.put(previous);

    let mut text_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut text_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { text, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    text.push_str("again");
    assert_reshape_miss(&cache, &text_changed);

    let mut font_size_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut font_size_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    style.as_mut().unwrap().font_size = 18.0;
    assert_reshape_miss(&cache, &font_size_changed);

    let width_changed = cache_fixture_input().with_max_width(96.0);
    assert_reshape_miss(&cache, &width_changed);

    let mut measured_size_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut measured_size_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::AtomicInlineBox { measurement, .. } = &mut children[1] else {
        panic!("expected atomic box");
    };
    measurement.measured_size = InlineIfcSize::new(36.0, 12.0);
    assert_reshape_miss(&cache, &measured_size_changed);
}

#[test]
fn cache_put_updates_entry_key_for_same_shape() {
    let mut cache = InlineIfcCache::new();
    let previous = cache_fixture_input();
    cache.put(previous);

    let mut next = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    style.as_mut().unwrap().brush = [10, 200, 10, 255];
    let next_key = next.cache_key();

    let updated = cache.put(next.clone());
    assert_eq!(updated.cache_key(), &next_key);
    assert_eq!(cache.len(), 1);

    let InlineIfcCacheLookup::Reuse(entry) = cache.lookup_input(&next) else {
        panic!("updated paint key should reuse on the next identical lookup");
    };
    assert_eq!(entry.cache_key(), &next_key);
}

#[test]
fn cache_update_reuses_same_input_without_rebuild() {
    let mut cache = InlineIfcCache::new();
    let input = cache_fixture_input();
    let expected_key = input.cache_key();
    cache.put(input.clone());

    {
        let update = cache.update(input);
        assert_eq!(update.invalidation, InlineIfcInvalidation::Reuse);
        assert!(!update.rebuilt);
        assert_eq!(update.entry.cache_key(), &expected_key);
        assert_eq!(update.entry.context().backing_text(), "cache me ");
    }
    assert_eq!(cache.len(), 1);
}

#[test]
fn cache_update_repaints_brush_only_change_and_refreshes_entry_key() {
    let mut cache = InlineIfcCache::new();
    let previous = cache_fixture_input();
    let previous_key = previous.cache_key();
    cache.put(previous);

    let mut next = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    style.as_mut().unwrap().brush = [240, 20, 20, 255];
    let next_key = next.cache_key();

    assert_eq!(previous_key.content, next_key.content);
    assert_eq!(previous_key.layout, next_key.layout);
    assert_ne!(previous_key.paint, next_key.paint);

    {
        let update = cache.update(next.clone());
        assert_eq!(update.invalidation, InlineIfcInvalidation::RepaintOnly);
        assert!(update.rebuilt);
        let entry_shape_key = InlineIfcShapeCacheKey::from_cache_key(update.entry.cache_key());
        assert_eq!(entry_shape_key.content, next_key.content);
        assert_eq!(entry_shape_key.layout, next_key.layout);
        assert_eq!(update.entry.cache_key(), &next_key);
    }
    assert_eq!(cache.len(), 1);

    let InlineIfcCacheLookup::Reuse(entry) = cache.lookup_input(&next) else {
        panic!("repaint update should refresh entry paint key");
    };
    assert_eq!(entry.cache_key(), &next_key);
}

#[test]
fn cache_update_reshapes_when_shape_inputs_change() {
    let mut cache = InlineIfcCache::new();
    cache.put(cache_fixture_input());

    let mut text_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut text_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { text, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    text.push_str("again");
    assert_cache_update_reshape(&mut cache, text_changed);

    let mut font_size_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut font_size_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { style, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    style.as_mut().unwrap().font_size = 18.0;
    assert_cache_update_reshape(&mut cache, font_size_changed);

    let width_changed = cache_fixture_input().with_max_width(96.0);
    assert_cache_update_reshape(&mut cache, width_changed);

    let mut measured_size_changed = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut measured_size_changed.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::AtomicInlineBox { measurement, .. } = &mut children[1] else {
        panic!("expected atomic box");
    };
    measurement.measured_size = InlineIfcSize::new(36.0, 12.0);
    assert_cache_update_reshape(&mut cache, measured_size_changed);
}

#[test]
fn cache_update_reuses_after_reshape_builds_new_entry() {
    let mut cache = InlineIfcCache::new();
    cache.put(cache_fixture_input());

    let mut next = cache_fixture_input();
    let InlineIfcItem::Span { children, .. } = &mut next.items[0] else {
        panic!("expected root span");
    };
    let InlineIfcItem::TextSpan { text, .. } = &mut children[0] else {
        panic!("expected text span");
    };
    text.push_str("again");
    let next_key = next.cache_key();

    {
        let update = cache.update(next.clone());
        assert_eq!(update.invalidation, InlineIfcInvalidation::Reshape);
        assert!(update.rebuilt);
        assert_eq!(update.entry.cache_key(), &next_key);
    }
    assert_eq!(cache.len(), 2);

    {
        let update = cache.update(next);
        assert_eq!(update.invalidation, InlineIfcInvalidation::Reuse);
        assert!(!update.rebuilt);
        assert_eq!(update.entry.cache_key(), &next_key);
    }
    assert_eq!(cache.len(), 2);
}

#[test]
fn cache_update_api_stays_within_ifc_boundary() {
    let mut cache = InlineIfcCache::new();
    let input = cache_fixture_input();
    let update = cache.update(input);

    assert_eq!(update.invalidation, InlineIfcInvalidation::Reshape);
    assert!(update.rebuilt);
    assert_eq!(update.entry.context().backing_text(), "cache me ");
    assert_eq!(update.entry.context().inline_boxes().len(), 1);
    assert!(!update.entry.context().text_paint_runs().is_empty());
    assert!(
        !update
            .entry
            .context()
            .decoration_paint_fragments()
            .is_empty()
    );
}

#[test]
fn cache_api_stores_ifc_context_without_render_dependencies() {
    let mut cache = InlineIfcCache::new();
    let input = cache_fixture_input();
    let entry = cache.put(input);

    assert_eq!(entry.context().backing_text(), "cache me ");
    assert_eq!(entry.context().inline_boxes().len(), 1);
    assert!(!entry.context().text_paint_runs().is_empty());
    assert!(!entry.context().decoration_paint_fragments().is_empty());
}
