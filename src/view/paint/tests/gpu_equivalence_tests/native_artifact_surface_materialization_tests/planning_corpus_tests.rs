use super::*;
use crate::style::{
    AnchorName, Angle, ClipMode, Opacity, Rotate, Scale, Transform, TransformOrigin, Translate,
};
use crate::view::paint::{
    PreparedArtifactSurfaceRasterPlan, PreparedArtifactSurfaceRasterStep,
    SurfaceDagExecutionTargetId,
};
use crate::view::test_support::{commit_child, commit_element, get_element_mut};

mod contracts_tests;
mod pixel_tests;
mod viewport_tests;

const EXTENT: [u32; 2] = [160, 128];
const RED: [u8; 4] = [255, 0, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const CLEAR: [u8; 4] = [0; 4];

#[derive(Clone, Copy, Debug)]
enum Scene {
    Scale,
    QuarterTurn,
    ObliqueTurn,
    NegativeOrigin,
    DeepForest,
    NamedAnchor,
    ClipScopes,
    ClipScopeSurface,
    ScrolledClipScopeSurface,
}
impl Scene {
    const ALL: [Self; 9] = [
        Self::Scale,
        Self::QuarterTurn,
        Self::ObliqueTurn,
        Self::NegativeOrigin,
        Self::DeepForest,
        Self::NamedAnchor,
        Self::ClipScopes,
        Self::ClipScopeSurface,
        Self::ScrolledClipScopeSurface,
    ];
}

struct Fixture {
    arena: NodeArena,
    roots: Vec<NodeKey>,
    // Recorded ownership order expected from fixture construction, not from
    // artifact store order. Empty owner chunks are included by the CPU gate.
    paint_owners: Vec<NodeKey>,
    probes: Vec<(&'static str, [u32; 2], [u8; 4])>,
}

fn style(size: [f32; 2], at: Option<[f32; 2]>, color: Option<[u8; 4]>) -> Style {
    let mut style = Style::new();
    style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Grid));
    style.insert(PropertyId::Width, ParsedValue::Length(Length::px(size[0])));
    style.insert(PropertyId::Height, ParsedValue::Length(Length::px(size[1])));
    if let Some([x, y]) = at {
        style.insert(
            PropertyId::Position,
            ParsedValue::Position(Position::absolute().left(Length::px(x)).top(Length::px(y))),
        );
    }
    if let Some([r, g, b, a]) = color {
        style.insert(
            PropertyId::BackgroundColor,
            ParsedValue::color_like(Color::rgba(r, g, b, a)),
        );
    }
    style
}

fn element(id: u64, size: [f32; 2], style: Style) -> Element {
    let mut element = Element::new_with_id(id, 0.0, 0.0, size[0], size[1]);
    element.apply_style(style);
    element
}

fn unlaid_out_fixture(scene: Scene) -> Fixture {
    let mut arena = new_test_arena();
    let root = commit_element(
        &mut arena,
        Box::new(element(
            0xc2_0000,
            [160.0, 128.0],
            style([160.0, 128.0], None, None),
        )),
    );
    let mut roots = vec![root];
    let mut paint_owners = Vec::new();
    let mut probes = Vec::new();
    match scene {
        Scene::Scale | Scene::QuarterTurn | Scene::ObliqueTurn | Scene::NegativeOrigin => {
            let at = if matches!(scene, Scene::NegativeOrigin) {
                [-8.0, 20.0]
            } else {
                [20.0, 20.0]
            };
            let mut s = style([20.0, 16.0], Some(at), Some(RED));
            s.set_transform_origin(TransformOrigin::px(0.0, 0.0));
            if matches!(scene, Scene::QuarterTurn | Scene::ObliqueTurn) {
                s.set_transform(Transform::new([
                    Translate::xy(Length::px(20.0), Length::px(0.0)),
                    Rotate::z(Angle::deg(if matches!(scene, Scene::ObliqueTurn) {
                        45.0
                    } else {
                        90.0
                    })),
                ]));
                if matches!(scene, Scene::ObliqueTurn) {
                    // The 45-degree quad has corners (40,20), (54.14,34.14),
                    // (42.83,45.46), (28.69,31.31). Both clear probes are
                    // inside its AABB but outside the actual textured quad.
                    probes.extend([
                        ("oblique interior", [42, 32], RED),
                        ("AABB above oblique edge", [52, 22], CLEAR),
                        ("AABB below oblique edge", [30, 44], CLEAR),
                    ]);
                } else {
                    probes.extend([
                        ("rotated interior", [30, 28], RED),
                        ("outside rotated right", [44, 28], CLEAR),
                        ("outside rotated bottom", [30, 44], CLEAR),
                    ]);
                }
            } else {
                s.set_transform(Transform::new([Scale::uniform(2.0)]));
                if matches!(scene, Scene::NegativeOrigin) {
                    // The interior only checks visible fill. The far/right
                    // probes constrain the output extent; none can prove
                    // negative-source content preservation for a uniform fill
                    // sampled with ClampToEdge, hiding the missing source texels.
                    probes.extend([
                        ("visible uniform-fill interior", [4, 28], RED),
                        ("scaled right extent remains filled", [28, 44], RED),
                        ("beyond scaled right extent", [36, 28], CLEAR),
                    ]);
                } else {
                    probes.extend([
                        ("scaled extension", [52, 44], RED),
                        ("outside scaled right", [64, 28], CLEAR),
                    ]);
                }
            }
            paint_owners.push(commit_child(
                &mut arena,
                root,
                Box::new(element(0xc2_0001, [20.0, 16.0], s)),
            ));
        }
        Scene::DeepForest => {
            let mut parent = root;
            for depth in 0..4 {
                let mut s = style(
                    [64.0, 64.0],
                    Some(if depth == 0 { [12.0, 12.0] } else { [2.0, 2.0] }),
                    None,
                );
                s.set_transform_origin(TransformOrigin::px(0.0, 0.0));
                s.set_transform(Transform::new([Scale::uniform(if depth % 2 == 0 {
                    2.0
                } else {
                    0.5
                })]));
                // A real isolated group shares an owner with a transform.
                if depth == 3 {
                    s.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
                }
                parent = commit_child(
                    &mut arena,
                    parent,
                    Box::new(element(0xc2_0010 + depth, [64.0, 64.0], s)),
                );
            }
            let red = commit_child(
                &mut arena,
                parent,
                Box::new(element(
                    0xc2_0020,
                    [48.0, 16.0],
                    style([48.0, 16.0], Some([0.0, 0.0]), Some(RED)),
                )),
            );
            let mut middle = style([28.0, 16.0], Some([12.0, 0.0]), Some(BLUE));
            middle.set_transform(Transform::new([Scale::uniform(1.25)]));
            middle.set_transform_origin(TransformOrigin::px(0.0, 0.0));
            let blue = commit_child(
                &mut arena,
                parent,
                Box::new(element(0xc2_0021, [28.0, 16.0], middle)),
            );
            let green = commit_child(
                &mut arena,
                parent,
                Box::new(element(
                    0xc2_0022,
                    [16.0, 16.0],
                    style([16.0, 16.0], Some([24.0, 0.0]), Some(GREEN)),
                )),
            );
            paint_owners.extend([red, blue, green]);
            // Four nested reference spaces accumulate translation 12+4+2+4=22,
            // and scales cancel to one. The final group opacity is 0.5.
            // Wide interiors avoid asserting opaque color inside the stacked
            // scale/downsample filters' edge footprint.
            probes.extend([
                ("deep red", [28, 30], [255, 0, 0, 128]),
                ("deep blue", [39, 30], [0, 0, 255, 128]),
                ("late green sibling", [54, 30], [0, 255, 0, 128]),
            ]);
            let mut second = style([20.0, 16.0], None, None);
            second.set_transform(Transform::new([Translate::xy(
                Length::px(104.0),
                Length::px(16.0),
            )]));
            second.set_background_image(
                Gradient::linear(SideOrCorner::Bottom)
                    .stop(Color::rgb(0, 255, 0), Some(Length::percent(0.0)))
                    .stop(Color::rgb(0, 255, 0), Some(Length::percent(100.0)))
                    .build(),
            );
            let second = commit_element(
                &mut arena,
                Box::new(element(0xc2_0030, [20.0, 16.0], second)),
            );
            roots.push(second);
            paint_owners.push(second);
            probes.push(("independent gradient root", [110, 22], GREEN));
        }
        Scene::NamedAnchor => {
            let mut anchor = element(
                0xc2_0040,
                [24.0, 20.0],
                style([24.0, 20.0], Some([60.0, 20.0]), Some(GREEN)),
            );
            anchor.set_anchor_name(Some(AnchorName::new("c2-reference")));
            let anchor = commit_child(&mut arena, root, Box::new(anchor));
            let mut s = style([12.0, 8.0], None, Some(RED));
            s.insert(
                PropertyId::Position,
                ParsedValue::Position(
                    Position::absolute()
                        .anchor("c2-reference")
                        .left(Length::px(4.0))
                        .top(Length::px(4.0)),
                ),
            );
            s.set_transform_origin(TransformOrigin::px(0.0, 0.0));
            s.set_transform(Transform::new([Scale::uniform(2.0)]));
            let anchored = commit_child(
                &mut arena,
                root,
                Box::new(element(0xc2_0041, [12.0, 8.0], s)),
            );
            paint_owners.extend([anchor, anchored]);
            probes.extend([
                ("anchor position", [62, 22], GREEN),
                ("anchored scaled leaf", [80, 32], RED),
                ("outside anchored leaf", [92, 32], CLEAR),
            ]);
        }
        Scene::ClipScopes | Scene::ClipScopeSurface | Scene::ScrolledClipScopeSurface => {
            let mut s = style([48.0, 40.0], Some([16.0, 16.0]), None);
            s.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            let outer = commit_child(
                &mut arena,
                root,
                Box::new(element(0xc2_0050, [48.0, 40.0], s)),
            );
            let mut s = style([40.0, 80.0], None, None);
            s.set_transform_origin(TransformOrigin::px(0.0, 0.0));
            s.set_transform(Transform::new([Scale::uniform(1.25)]));
            let content = commit_child(
                &mut arena,
                outer,
                Box::new(element(0xc2_0051, [40.0, 80.0], s)),
            );
            let mut s = style([24.0, 20.0], Some([4.0, 4.0]), None);
            s.insert(
                PropertyId::ScrollDirection,
                ParsedValue::ScrollDirection(ScrollDirection::Vertical),
            );
            let inner = commit_child(
                &mut arena,
                content,
                Box::new(element(0xc2_0052, [24.0, 20.0], s)),
            );
            let leaf = commit_child(
                &mut arena,
                inner,
                Box::new(element(
                    0xc2_0053,
                    [24.0, 60.0],
                    style([24.0, 60.0], None, Some(RED)),
                )),
            );
            paint_owners.push(leaf);
            if matches!(scene, Scene::ScrolledClipScopeSurface) {
                get_element_mut::<Element>(&arena, inner).set_scroll_offset((0.0, 8.0));
                // The retained inner scroll must not capture this overflow
                // surface, yet the recorded placement still includes scroll.
                // Its 8 logical px displacement becomes 10 after parent scale.
                // The displaced bands are wider than both scale filters, so
                // these probes test full content/clear coverage, not AA edges.
                probes.extend([
                    ("scrolled escape moved upward", [59, 23], [0, 0, 255, 128]),
                    ("scrolled escape vacated bottom", [59, 35], CLEAR),
                ]);
            }
            // An overflow-late Replace scope returns to its grandparent's clip.
            let mut s = style([8.0, 8.0], None, Some(BLUE));
            s.insert(
                PropertyId::Position,
                ParsedValue::Position(
                    Position::absolute()
                        .left(Length::px(28.0))
                        .top(Length::px(4.0))
                        .clip(ClipMode::AnchorParent),
                ),
            );
            if matches!(
                scene,
                Scene::ClipScopeSurface | Scene::ScrolledClipScopeSurface
            ) {
                s.set_transform_origin(TransformOrigin::px(0.0, 0.0));
                s.set_transform(Transform::new([Scale::uniform(1.25)]));
                s.insert(PropertyId::Opacity, ParsedValue::Opacity(Opacity::new(0.5)));
            }
            let escape = commit_child(
                &mut arena,
                inner,
                Box::new(element(0xc2_0054, [8.0, 8.0], s)),
            );
            paint_owners.push(escape);
            let blue = if matches!(
                scene,
                Scene::ClipScopeSurface | Scene::ScrolledClipScopeSurface
            ) {
                [0, 0, 255, 128]
            } else {
                BLUE
            };
            probes.extend([
                ("nested scroll clip interior", [26, 26], RED),
                ("below inner clip", [26, 50], CLEAR),
                (
                    "replace scope escape",
                    [
                        59,
                        if matches!(scene, Scene::ScrolledClipScopeSurface) {
                            24
                        } else {
                            29
                        },
                    ],
                    blue,
                ),
                ("outside outer clip", [70, 29], CLEAR),
            ]);
        }
    }
    Fixture {
        arena,
        roots,
        paint_owners,
        probes,
    }
}

fn fixture(scene: Scene) -> Fixture {
    let mut fixture = unlaid_out_fixture(scene);
    let mut layout = Viewport::new();
    for &root in &fixture.roots {
        crate::view::viewport::layout_artifact_style_scene_for_test(
            &mut layout,
            &mut fixture.arena,
            root,
            EXTENT.map(|v| v as f32),
        );
    }
    fixture
}

fn record(fixture: &Fixture) -> PaintArtifact {
    let (properties, generations) = sync_identity(&fixture.arena, &fixture.roots);
    let FrameArtifactRecordOutcome::Artifact { artifact, .. } = record_surface_dag_frame_artifact(
        &fixture.arena,
        &fixture.roots,
        &properties,
        &generations,
        RendererMode::ForcedForTests,
    )
    .expect("complete production layout must record") else {
        panic!("fallback")
    };
    artifact
}

fn prepare(
    artifact: PaintArtifact,
    dpr: f32,
    offset: [f32; 2],
) -> PreparedArtifactSurfaceRasterPlan {
    prepare_artifact_surface_raster_plan(
        artifact,
        ArtifactSurfaceRasterContext::new(dpr, FORMAT, offset, None, 8192, 128 * 1024 * 1024)
            .unwrap(),
    )
    .expect("generic plan")
}
