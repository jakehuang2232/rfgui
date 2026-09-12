use super::*;

fn wrapper_style(opacity: f32) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(20.0)));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(32.0)));
    style.insert(
        PropertyId::BackgroundColor,
        ParsedValue::color_like(Color::rgb(255, 0, 0)),
    );
    style.insert(
        PropertyId::Opacity,
        ParsedValue::Opacity(Opacity::new(opacity)),
    );
    style
}

#[test]
fn generic_resource_wrapper_state_preserves_effects_and_rejects_wrong_authority() {
    for svg_host in [false, true] {
        for error in [false, true] {
            let mut arena = new_test_arena();
            let mut root = Element::new_with_id(0xc1_2000, 0.0, 0.0, 20.0, 32.0);
            let mut style = wrapper_style(0.75);
            style.insert(PropertyId::Height, ParsedValue::Length(Length::px(16.0)));
            style.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            root.apply_style(style);
            let root = commit_element(&mut arena, Box::new(root));
            let mut host_style = wrapper_style(0.5);
            host_style.set_transform(Transform::new([Translate::xy(
                Length::px(3.0),
                Length::px(4.0),
            )]));
            let owner = if svg_host {
                let source = SvgSource::Content(format!(
                    "<svg xmlns=\"http://www.w3.org/2000/svg\"><!-- c12 {error} --></svg>"
                ));
                let key = crate::view::svg_resource::prime_svg_document_ready_for_test(
                    &source, 20.0, 32.0,
                );
                if error {
                    crate::view::svg_resource::set_svg_document_error_for_test(key);
                } else {
                    crate::view::svg_resource::set_svg_document_loading_for_test(key);
                }
                let mut svg = Svg::new_with_id(0xc1_2001, source);
                svg.apply_style(host_style);
                commit_child(&mut arena, root, Box::new(svg))
            } else {
                let source = ImageSource::Rgba {
                    width: 1,
                    height: 1,
                    pixels: Arc::from([255, 0, 0, 255]),
                };
                let handle = crate::view::image_resource::acquire_image_resource(&source);
                if error {
                    crate::view::image_resource::set_image_error_for_test(handle.asset_id(), "c12");
                } else {
                    crate::view::image_resource::set_image_loading_for_test(handle.asset_id());
                }
                let mut image = Image::new_with_id(0xc1_2001, source);
                image.apply_style(host_style);
                commit_child(&mut arena, root, Box::new(image))
            };
            let child = arena.insert(Node::with_parent(
                Box::new(Element::new_with_id(0xc1_2002, 0.0, 0.0, 8.0, 8.0)),
                Some(owner),
            ));
            arena.with_element_taken(owner, |element, _| {
                if svg_host {
                    let svg = element.as_any_mut().downcast_mut::<Svg>().unwrap();
                    if error {
                        svg.attach_error_slot_cold(vec![child]);
                    } else {
                        svg.attach_loading_slot_cold(vec![child]);
                    }
                } else {
                    let image = element.as_any_mut().downcast_mut::<Image>().unwrap();
                    if error {
                        image.attach_error_slot_cold(vec![child]);
                    } else {
                        image.attach_loading_slot_cold(vec![child]);
                    }
                }
            });
            let mut layout_viewport = crate::view::viewport::Viewport::new();
            crate::view::viewport::layout_artifact_style_scene_for_test(
                &mut layout_viewport,
                &mut arena,
                root,
                [64.0, 64.0],
            );
            let (properties, generations) = sync_identity(&arena, &[root]);
            let outcome = record_surface_dag_frame_artifact(
                &arena,
                &[root],
                &properties,
                &generations,
                RendererMode::ForcedForTests,
            )
            .expect("complete resource wrapper state must record");
            let FrameArtifactRecordOutcome::Artifact {
                artifact,
                eligibility,
            } = outcome
            else {
                panic!("generic fallback");
            };
            assert!(eligibility.eligible);
            let state = properties.paint_state_for(owner).unwrap();
            assert!(
                state.effect.is_some()
                    && state.transform.is_some()
                    && state.scroll.is_some()
                    && state.clip.is_some()
            );
            let chunk = artifact
                .chunks
                .iter()
                .find(|chunk| chunk.owner == owner)
                .unwrap();
            assert_eq!(chunk.properties, state);
            assert!(
                artifact.ops[chunk.op_range.clone()]
                    .iter()
                    .any(|op| matches!(op,PaintOp::DrawRect(rect) if rect.params.opacity==0.5)),
                "record local opacity, not neutral or ancestor product"
            );
            let local_effect = artifact
                .effect_nodes
                .iter()
                .find(|effect| effect.owner == owner)
                .unwrap();
            let ancestor_effect = artifact
                .effect_nodes
                .iter()
                .find(|effect| effect.owner == root)
                .unwrap();
            assert_eq!(local_effect.opacity, 0.5);
            assert_eq!(ancestor_effect.opacity, 0.75);
            assert_eq!(local_effect.parent, Some(ancestor_effect.id));
            assert_eq!(state.effect, Some(local_effect.id));
            // Exercise the actual metadata hook with the accepted context, then
            // alter one authority field at a time. No property-family allowlist.
            let context = PaintRecordingContext {
                recording_owner: Some(owner),
                recording_owner_stable_id: Some(0xc1_2001),
                surface_dag: true,
                surface_dag_paint_state: Some(state),
                surface_dag_transform: state.transform,
                ..Default::default()
            };
            let node = arena.get(owner).unwrap();
            let record = |ctx| {
                node.element.record_shadow_paint_metadata(
                    owner,
                    state,
                    chunk.content_revision,
                    &arena,
                    &ctx,
                )
            };
            assert!(record(context).is_some());
            assert!(
                record(PaintRecordingContext {
                    recording_owner: Some(root),
                    ..context
                })
                .is_none()
            );
            assert!(
                record(PaintRecordingContext {
                    recording_owner_stable_id: Some(0),
                    ..context
                })
                .is_none()
            );
            assert!(
                record(PaintRecordingContext {
                    surface_dag_paint_state: None,
                    ..context
                })
                .is_none()
            );
            assert!(
                record(PaintRecordingContext {
                    surface_dag: false,
                    ..context
                })
                .is_none()
            );
            let mut wrong = state;
            wrong.effect = properties.paint_state_for(root).unwrap().effect;
            assert_ne!(wrong, state);
            assert!(
                record(PaintRecordingContext {
                    surface_dag_paint_state: Some(wrong),
                    ..context
                })
                .is_none()
            );
        }
    }
}
