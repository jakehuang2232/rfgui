use super::*;

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn public_native_hosts_close_retained_auto_metadata_and_full_recording() {
    let (text_area_arena, text_area_roots, _) = prepared_auto_text_area(0.0, false);
    let fixtures: [(&str, (NodeArena, Vec<NodeKey>)); 5] = [
        ("Element", prepared_safe_leaf()),
        ("Text", prepared_native_text()),
        ("TextArea", (text_area_arena, text_area_roots)),
        ("Image", prepared_native_image()),
        ("Svg", prepared_native_svg()),
    ];

    for (host, (arena, roots)) in fixtures {
        assert_native_host_retained_closure(host, &arena, &roots);
    }
}

#[test]
fn retained_auto_treats_empty_text_as_a_transparent_native_leaf() {
    let (arena, roots) = prepared_empty_native_text();
    assert_native_host_retained_closure("empty Text", &arena, &roots);
}

#[test]
fn transparent_native_text_root_uses_generic_surface_artifact() {
    let (arena, roots, _) = prepared_transparent_native_text();
    assert_native_root_opacity_artifact("transparent Text", &arena, &roots, 0.0);
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn retained_auto_final_authority_covers_native_transform_effect_and_root_opacity() {
    let mut emitted_transform = false;
    for host in ["Image", "Svg"] {
        for state in ["ready", "loading", "error"] {
            let (arena, roots) = prepared_native_media_transform(host, state);
            let emit = !emitted_transform && host == "Image" && state == "ready";
            assert_native_artifact_surface_authority(
                &format!("direct {host} transform {state}"),
                &arena,
                &roots,
                emit,
            );
            emitted_transform |= emit;
        }
    }
    assert!(emitted_transform);

    let mut emitted_effect = false;
    for (host, states) in [
        ("Text", &["ready"][..]),
        ("Image", &["ready", "loading", "error"][..]),
        ("Svg", &["ready", "loading", "error"][..]),
    ] {
        for state in states {
            let (arena, roots, _child) = prepared_nested_native_effect(host, state);
            let emit = !emitted_effect && host == "Text";
            // C-1.2: complete generic wrapper state records Loading/Error too.
            assert_native_artifact_surface_authority(
                &format!("nested {host} effect {state}"),
                &arena,
                &roots,
                emit,
            );
            emitted_effect |= emit;
        }
    }
    assert!(emitted_effect);

    let (arena, roots) = prepared_native_text_with_opacity(0.5);
    assert_native_root_opacity_artifact("root Text opacity", &arena, &roots, 0.5);
    let (arena, roots) = prepared_native_image_with_opacity(0.5);
    assert_native_root_opacity_artifact("root Image opacity", &arena, &roots, 0.5);
    let (arena, roots) = prepared_native_svg_with_opacity(0.5);
    assert_native_root_opacity_artifact("root Svg opacity", &arena, &roots, 0.5);
}

#[test]
fn native_text_image_svg_root_opacity_selects_and_compiles_retained_auto() {
    for opacity in [0.0, 0.5, 1.0] {
        let (arena, roots) = prepared_native_text_with_opacity(opacity);
        assert_native_root_opacity_artifact("Text", &arena, &roots, opacity);

        let (arena, roots) = prepared_native_image_with_opacity(opacity);
        assert_native_root_opacity_artifact("Image ready", &arena, &roots, opacity);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let (arena, roots) = prepared_native_svg_with_opacity(opacity);
            assert_native_root_opacity_artifact("Svg ready", &arena, &roots, opacity);
        }
    }
}

#[test]
fn native_image_svg_loading_error_root_opacity_stays_retained_auto() {
    for error in [false, true] {
        let state = if error { "error" } else { "loading" };
        let (arena, roots) =
            prepared_native_image_path_state(&format!("image-{state}"), 0.5, error);
        assert_native_root_opacity_artifact(&format!("Image {state}"), &arena, &roots, 0.5);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let (arena, roots) =
                prepared_native_svg_path_state(&format!("svg-{state}"), 0.5, error);
            assert_native_root_opacity_artifact(&format!("Svg {state}"), &arena, &roots, 0.5);
        }
    }
}
