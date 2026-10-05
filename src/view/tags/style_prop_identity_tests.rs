use super::*;
use crate::style::Color;
use crate::ui::{Patch, reconcile, rsx};

fn element_style(width: f32, color: &'static str) -> ElementStylePropSchema {
    ElementStylePropSchema {
        width: Some(Length::px(width)),
        background_color: Some(Box::new(Color::hex(color))),
        hover: Some(HoverElementStylePropSchema {
            opacity: Some(Opacity::new(0.5)),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn text_style(color: &'static str) -> TextStylePropSchema {
    TextStylePropSchema {
        color: Some(Box::new(Color::hex(color))),
        font_size: Some(FontSize::px(15.0)),
        ..Default::default()
    }
}

#[test]
fn host_style_props_compare_by_lowered_value_across_allocations() {
    let a = element_style(120.0, "#123456").into_prop_value();
    let b = element_style(120.0, "#123456").into_prop_value();
    assert_eq!(
        a, b,
        "separately built equal styles are the same prop value"
    );
    assert_ne!(a, element_style(121.0, "#123456").into_prop_value());
    assert_ne!(a, element_style(120.0, "#654321").into_prop_value());

    let text = text_style("#abcdef").into_prop_value();
    assert_eq!(text, text_style("#abcdef").into_prop_value());
    assert_ne!(text, text_style("#fedcba").into_prop_value());
    assert_ne!(
        a, text,
        "different schemas never compare equal, even when both lower to a style"
    );
}

#[test]
fn host_style_props_decode_schema_and_cached_lowering() {
    let value = element_style(120.0, "#123456").into_prop_value();
    assert_eq!(
        crate::view::tags::lowered_host_style::<ElementStylePropSchema>(&value),
        Some(element_style(120.0, "#123456").to_style())
    );
    assert_eq!(
        crate::view::tags::lowered_host_style::<TextStylePropSchema>(&value),
        None
    );
    let schema = ElementStylePropSchema::from_prop_value(value).expect("schema round trip");
    assert_eq!(
        schema.to_style(),
        element_style(120.0, "#123456").to_style()
    );
}

#[test]
fn opaque_shared_props_keep_allocation_identity() {
    let source = || ImageSource::Path("logo.png".into()).into_prop_value();
    let value = source();
    assert_eq!(value, value.clone());
    assert_ne!(source(), source());
}

#[test]
fn reconcile_emits_no_patch_for_a_rebuilt_equal_style() {
    let tree = |width: f32| {
        rsx! {
            <Element style={{ width: Length::px(width), background_color: Color::hex("#123456") }}>
                <Text style={{ color: Color::hex("#abcdef") }}>"label"</Text>
            </Element>
        }
    };
    let old = tree(100.0);
    assert!(
        reconcile(Some(&old), &tree(100.0)).is_empty(),
        "a rebuilt tree with equal styles needs no commit work"
    );
    let patches = reconcile(Some(&old), &tree(101.0));
    assert!(
        matches!(
            patches.as_slice(),
            [Patch::UpdateElementProps { path, changed, removed }]
                if path.is_empty() && removed.is_empty()
                    && changed.iter().map(|(key, _)| *key).eq(["style"])
        ),
        "only the changed root style is patched: {patches:?}"
    );
}
