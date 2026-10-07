//! Rigidly translating a subtree changes only the relative edge of its root.
//! Every other retained-pipeline input of the subtree — property snapshots
//! and recorded chunks — must be identical, because relative-to-absolute
//! conversion happens once, in the compiler.
use super::retained_acceptance_fixtures::*;
use super::*;
use crate::view::compositor::property_tree::{
    ClipNodeId, EffectNodeId, LayoutPositionNodeId, ScrollNodeId, TransformNodeId,
    VisualOffsetNodeId,
};
use crate::view::test_support::get_element_mut;

const HOST: [f32; 2] = [420.0, 360.0];

struct Moved {
    arena: NodeArena,
    host: NodeKey,
    roots: Vec<NodeKey>,
    viewport: crate::view::viewport::Viewport,
}

fn hosted(scene: Scene) -> Moved {
    let fixture = unlaid_out_fixture(scene);
    let mut arena = fixture.arena;
    let host = commit_element(
        &mut arena,
        Box::new(element(0xc9_0000, HOST, style(HOST, None, None))),
    );
    for &root in &fixture.roots {
        arena.push_child(host, root);
        arena.set_parent(root, Some(host));
    }
    Moved {
        arena,
        host,
        roots: fixture.roots,
        viewport: crate::view::viewport::Viewport::new(),
    }
}

impl Moved {
    fn place(&mut self, at: [f32; 2]) -> (PropertyTrees, PaintArtifact) {
        for &root in &self.roots {
            let size = {
                let node = self.arena.get(root).unwrap();
                let snapshot = node.element.box_model_snapshot();
                [snapshot.width, snapshot.height]
            };
            get_element_mut::<Element>(&self.arena, root).apply_style(style(size, Some(at), None));
        }
        crate::view::viewport::layout_artifact_style_scene_for_test(
            &mut self.viewport,
            &mut self.arena,
            self.host,
            HOST,
        );
        let (properties, generations) = sync_identity(&self.arena, &[self.host]);
        let FrameArtifactRecordOutcome::Artifact { artifact, .. } =
            record_surface_dag_frame_artifact(
                &self.arena,
                &[self.host],
                &properties,
                &generations,
                RendererMode::ForcedForTests,
            )
            .expect("record")
        else {
            panic!("fallback")
        };
        (properties, artifact)
    }

    fn subtree(&self) -> Vec<NodeKey> {
        let mut out = Vec::new();
        let mut pending = self.roots.clone();
        while let Some(key) = pending.pop() {
            out.push(key);
            pending.extend(self.arena.children_of(key));
        }
        out
    }
}

/// One retained-pipeline input that changed under translation.
struct Variant {
    kind: &'static str,
    detail: String,
}

impl std::fmt::Debug for Variant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.kind, self.detail)
    }
}

/// Every retained-pipeline input of the moved subtree that changed, other
/// than the moved roots' own layout edges.
fn translated_inputs(scene: Scene, from: [f32; 2], to: [f32; 2]) -> Vec<Variant> {
    let mut moved = hosted(scene);
    let (before_trees, before) = moved.place(from);
    let (after_trees, after) = moved.place(to);
    let mut diffs = Vec::new();
    for key in moved.subtree() {
        let root = moved.roots.contains(&key);
        let mut compare = |kind: &'static str, a: String, b: String| {
            if a != b {
                diffs.push(Variant {
                    kind,
                    detail: format!("{key:?}: {}", changed_tokens(&a, &b)),
                });
            }
        };
        if !root {
            compare(
                "layout_position",
                format!(
                    "{:?}",
                    before_trees.layout_position_snapshot_for(LayoutPositionNodeId(key))
                ),
                format!(
                    "{:?}",
                    after_trees.layout_position_snapshot_for(LayoutPositionNodeId(key))
                ),
            );
        }
        compare(
            "visual_offset",
            format!(
                "{:?}",
                before_trees.visual_offset_snapshot_for(VisualOffsetNodeId(key))
            ),
            format!(
                "{:?}",
                after_trees.visual_offset_snapshot_for(VisualOffsetNodeId(key))
            ),
        );
        compare(
            "transform",
            format!(
                "{:?}",
                before_trees.transform_snapshot_for(TransformNodeId(key))
            ),
            format!(
                "{:?}",
                after_trees.transform_snapshot_for(TransformNodeId(key))
            ),
        );
        compare(
            "effect",
            format!(
                "{:?}",
                before_trees.effect_node_snapshot_for(EffectNodeId(key))
            ),
            format!(
                "{:?}",
                after_trees.effect_node_snapshot_for(EffectNodeId(key))
            ),
        );
        compare(
            "scroll",
            format!("{:?}", before_trees.scroll_snapshot_for(ScrollNodeId(key))),
            format!("{:?}", after_trees.scroll_snapshot_for(ScrollNodeId(key))),
        );
        for role in [
            crate::view::compositor::property_tree::ClipNodeRole::SelfClip,
            crate::view::compositor::property_tree::ClipNodeRole::ContentsClip,
        ] {
            let id = ClipNodeId { owner: key, role };
            compare(
                "clip",
                format!("{:?}", before_trees.clip_node_snapshot_for(id)),
                format!("{:?}", after_trees.clip_node_snapshot_for(id)),
            );
        }
    }
    let chunks = |artifact: &PaintArtifact| {
        artifact
            .chunks
            .iter()
            .map(|chunk| {
                (
                    chunk.id,
                    format!(
                        "bounds={:?} payload={:?} ops={:?}",
                        chunk.bounds,
                        chunk.payload_identity,
                        &artifact.ops[chunk.op_range.clone()]
                    ),
                )
            })
            .collect::<Vec<_>>()
    };
    let (before, after) = (chunks(&before), chunks(&after));
    if before.len() != after.len() {
        diffs.push(Variant {
            kind: "chunk",
            detail: format!("count {} -> {}", before.len(), after.len()),
        });
    }
    for ((id, a), (other, b)) in before.iter().zip(&after) {
        if id != other {
            diffs.push(Variant {
                kind: "chunk",
                detail: format!("order {id:?} -> {other:?}"),
            });
        } else if a != b {
            diffs.push(Variant {
                kind: "chunk",
                detail: format!("{:?}/{:?}: {}", id.owner, id.role, changed_tokens(a, b)),
            });
        }
    }
    diffs
}

/// The `field: value` tokens of two same-shaped Debug strings that differ.
fn changed_tokens(a: &str, b: &str) -> String {
    let split = |s: &str| {
        s.split(|c| {
            c == ',' || c == '{' || c == '}' || c == '(' || c == ')' || c == '[' || c == ']'
        })
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_owned)
        .collect::<Vec<_>>()
    };
    let (a, b) = (split(a), split(b));
    if a.len() != b.len() {
        return format!("shape {} -> {} tokens", a.len(), b.len());
    }
    let mut out = Vec::new();
    let mut last_label = String::new();
    for (x, y) in a.iter().zip(&b) {
        if let Some((label, _)) = x.split_once(':') {
            last_label = label.to_owned();
        }
        if x != y {
            let token = if x.contains(':') {
                x.split_once(':').unwrap().0.to_owned()
            } else {
                format!("{last_label}[..]")
            };
            if !out.contains(&token) {
                out.push(token);
            }
        }
    }
    out.join(" ")
}

const MOVES: [([f32; 2], [f32; 2]); 2] =
    [([20.0, 10.0], [27.0, 13.0]), ([20.0, 10.0], [22.5, 10.25])];

/// Input families that are already relative and must stay invariant.
const INVARIANT: [&str; 6] = [
    "layout_position",
    "visual_offset",
    "transform",
    "effect",
    "clip",
    "scroll",
];

#[test]
fn translation_keeps_relative_property_snapshots() {
    for scene in Scene::ALL {
        for (from, to) in MOVES {
            let variant = translated_inputs(scene, from, to)
                .into_iter()
                .filter(|variant| INVARIANT.contains(&variant.kind))
                .collect::<Vec<_>>();
            assert!(
                variant.is_empty(),
                "{scene:?} {from:?}->{to:?}: {variant:#?}"
            );
        }
    }
}

#[test]
#[ignore = "inventory: prints translation-variant retained inputs"]
fn inventory_translation_variant_inputs() {
    for scene in Scene::ALL {
        let diffs = translated_inputs(scene, MOVES[0].0, MOVES[0].1);
        eprintln!("=== {scene:?}: {} variant inputs", diffs.len());
        for diff in diffs.iter().take(40) {
            eprintln!("  {diff:?}");
        }
    }
}

/// The spatial graph's relative edges compose back to every owner's layout
/// position bit for bit, so the compiler can be the sole relative-to-absolute
/// conversion.
fn assert_layout_positions_reproduced(label: &str, arena: &NodeArena, trees: &PropertyTrees) {
    use crate::view::compositor::property_tree::SpatialProjectionGraph;
    let transforms = trees
        .transforms
        .keys()
        .filter_map(|id| trees.transform_snapshot_for(*id))
        .collect::<Vec<_>>();
    let positions = trees
        .layout_positions
        .keys()
        .filter_map(|id| trees.layout_position_snapshot_for(*id))
        .collect::<Vec<_>>();
    let visuals = trees
        .visual_offsets
        .keys()
        .filter_map(|id| trees.visual_offset_snapshot_for(*id))
        .collect::<Vec<_>>();
    let scrolls = trees
        .scrolls
        .keys()
        .filter_map(|id| trees.scroll_snapshot_for(*id))
        .collect::<Vec<_>>();
    let graph = SpatialProjectionGraph::try_new(&transforms, &positions, &visuals, &scrolls)
        .unwrap_or_else(|error| panic!("{label}: {error:?}"));
    assert!(
        !positions.is_empty(),
        "{label}: fixture has layout positions"
    );
    for snapshot in &positions {
        let owner = snapshot.owner;
        let layout = arena.get(owner).unwrap().element.box_model_snapshot();
        let derived = graph
            .derive_optional_owner_viewport_position(owner)
            .unwrap_or_else(|error| panic!("{label} {owner:?}: {error:?}"));
        assert_eq!(
            derived.to_array().map(f32::to_bits),
            [layout.x, layout.y].map(f32::to_bits),
            "{label} {owner:?}: derived {derived:?} vs layout ({}, {})",
            layout.x,
            layout.y
        );
    }
}

fn wrapped_inline_atomics() -> (NodeArena, NodeKey) {
    let mut arena = new_test_arena();
    let host = commit_element(
        &mut arena,
        Box::new(element(0xc9_1000, HOST, style(HOST, None, None))),
    );
    let mut root_style = style([96.0, 120.0], Some([13.0, 7.5]), None);
    root_style.insert(PropertyId::Layout, ParsedValue::Layout(Layout::Inline));
    let root = commit_child(
        &mut arena,
        host,
        Box::new(element(0xc9_1001, [96.0, 120.0], root_style)),
    );
    commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(
            0xc9_1002,
            0.0,
            0.0,
            0.0,
            0.0,
            "words that wrap ",
        )),
    );
    commit_child(
        &mut arena,
        root,
        Box::new(element(
            0xc9_1003,
            [30.0, 14.0],
            style([30.0, 14.0], None, Some(RED)),
        )),
    );
    commit_child(
        &mut arena,
        root,
        Box::new(Text::new_with_id(
            0xc9_1004,
            0.0,
            0.0,
            0.0,
            0.0,
            " and more text after it ",
        )),
    );
    let nested = commit_child(
        &mut arena,
        root,
        Box::new(element(
            0xc9_1005,
            [40.0, 18.0],
            style([40.0, 18.0], None, Some(BLUE)),
        )),
    );
    commit_child(
        &mut arena,
        nested,
        Box::new(element(
            0xc9_1006,
            [8.0, 6.0],
            style([8.0, 6.0], None, Some(GREEN)),
        )),
    );
    (arena, host)
}

#[test]
fn spatial_graph_reproduces_every_layout_position() {
    for scene in Scene::ALL {
        let mut moved = hosted(scene);
        let (trees, _) = moved.place([20.0, 10.0]);
        assert_layout_positions_reproduced(&format!("{scene:?}"), &moved.arena, &trees);
    }
    let (mut arena, host) = wrapped_inline_atomics();
    let mut viewport = crate::view::viewport::Viewport::new();
    crate::view::viewport::layout_artifact_style_scene_for_test(
        &mut viewport,
        &mut arena,
        host,
        HOST,
    );
    let (trees, _) = sync_identity(&arena, &[host]);
    assert_layout_positions_reproduced("wrapped inline atomics", &arena, &trees);
}

/// Placing an owner-local self clip at its owner's derived frame reproduces
/// the scissor legacy paint applies from the owner's live absolute geometry.
#[test]
fn placed_self_clips_reproduce_live_scissors() {
    use crate::view::compositor::property_tree::ClipNodeRole;
    let mut checked = 0;
    for scene in Scene::ALL {
        for (_, at) in MOVES {
            let mut moved = hosted(scene);
            let (_, mut artifact) = moved.place(at);
            let local = artifact.clip_nodes.clone();
            crate::view::paint::ArtifactSpatialProjection::try_new(&artifact)
                .and_then(|spatial| spatial.place_clips(&mut artifact.clip_nodes))
                .unwrap_or_else(|error| panic!("{scene:?} {at:?}: {error:?}"));
            for (clip, placed) in local.iter().zip(&artifact.clip_nodes) {
                if clip.id.role != ClipNodeRole::SelfClip || !clip.geometry.is_owner_local() {
                    continue;
                }
                let node = moved.arena.get(clip.owner).unwrap();
                let Some(element) = node.element.as_any().downcast_ref::<Element>() else {
                    continue;
                };
                assert_eq!(
                    placed.geometry.viewport_scissor(),
                    element.absolute_clip_scissor_rect(),
                    "{scene:?} {at:?} {:?}",
                    clip.owner
                );
                checked += 1;
            }
        }
    }
    assert!(checked > 0, "the corpus carries owner-local self clips");
}
