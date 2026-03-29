use crate::ui::{PropValue, RsxElementNode, RsxNode, RsxNodeIdentity};
use std::collections::{HashMap, VecDeque};

#[derive(Clone, Debug, PartialEq)]
pub enum Patch {
    ReplaceRoot(RsxNode),
    ReplaceNode {
        path: Vec<usize>,
        node: RsxNode,
    },
    UpdateElementProps {
        path: Vec<usize>,
        props: Vec<(String, PropValue)>,
    },
    SetText {
        path: Vec<usize>,
        text: String,
    },
    InsertChild {
        parent_path: Vec<usize>,
        index: usize,
        node: RsxNode,
    },
    RemoveChild {
        parent_path: Vec<usize>,
        index: usize,
    },
    MoveChild {
        parent_path: Vec<usize>,
        from: usize,
        to: usize,
    },
}

pub fn reconcile(old: Option<&RsxNode>, new: &RsxNode) -> Vec<Patch> {
    let Some(old) = old else {
        return vec![Patch::ReplaceRoot(new.clone())];
    };

    let mut patches = Vec::new();
    reconcile_node(old, new, &[], &mut patches);
    patches
}

fn reconcile_node(old: &RsxNode, new: &RsxNode, path: &[usize], patches: &mut Vec<Patch>) {
    if old.identity() != new.identity() {
        if path.is_empty() {
            patches.push(Patch::ReplaceRoot(new.clone()));
        } else {
            patches.push(Patch::ReplaceNode {
                path: path.to_vec(),
                node: new.clone(),
            });
        }
        return;
    }

    match (old, new) {
        (RsxNode::Element(old_node), RsxNode::Element(new_node)) => {
            reconcile_element(old_node, new_node, path, patches);
        }
        (RsxNode::Text(old_text), RsxNode::Text(new_text)) => {
            if old_text.content != new_text.content {
                patches.push(Patch::SetText {
                    path: path.to_vec(),
                    text: new_text.content.clone(),
                });
            }
        }
        (RsxNode::Fragment(old_fragment), RsxNode::Fragment(new_fragment)) => {
            reconcile_children(
                &old_fragment.children,
                &new_fragment.children,
                path,
                patches,
            );
        }
        _ => {
            if path.is_empty() {
                patches.push(Patch::ReplaceRoot(new.clone()));
            } else {
                patches.push(Patch::ReplaceNode {
                    path: path.to_vec(),
                    node: new.clone(),
                });
            }
        }
    }
}

fn reconcile_element(
    old: &RsxElementNode,
    new: &RsxElementNode,
    path: &[usize],
    patches: &mut Vec<Patch>,
) {
    let same_tag = match (old.tag_descriptor, new.tag_descriptor) {
        (Some(old_desc), Some(new_desc)) => old_desc == new_desc,
        _ => old.tag == new.tag,
    };

    if !same_tag {
        if path.is_empty() {
            patches.push(Patch::ReplaceRoot(RsxNode::Element(new.clone())));
        } else {
            patches.push(Patch::ReplaceNode {
                path: path.to_vec(),
                node: RsxNode::Element(new.clone()),
            });
        }
        return;
    }

    if old.props != new.props {
        patches.push(Patch::UpdateElementProps {
            path: path.to_vec(),
            props: new.props.clone(),
        });
    }

    reconcile_children(&old.children, &new.children, path, patches);
}

fn reconcile_children(
    old_children: &[RsxNode],
    new_children: &[RsxNode],
    parent_path: &[usize],
    patches: &mut Vec<Patch>,
) {
    let mut old_keyed = HashMap::<RsxNodeIdentity, usize>::new();
    let mut old_unkeyed = HashMap::<String, VecDeque<usize>>::new();
    for (index, child) in old_children.iter().enumerate() {
        let identity = child.identity().clone();
        if identity.key.is_some() {
            old_keyed.insert(identity, index);
        } else {
            old_unkeyed
                .entry(identity.invocation_type)
                .or_default()
                .push_back(index);
        }
    }

    let mut matches = Vec::<Option<usize>>::with_capacity(new_children.len());
    let mut matched_old = vec![false; old_children.len()];
    for new_child in new_children {
        let identity = new_child.identity();
        let matched = if identity.key.is_some() {
            old_keyed.remove(identity)
        } else {
            old_unkeyed
                .get_mut(&identity.invocation_type)
                .and_then(VecDeque::pop_front)
        };
        if let Some(old_index) = matched {
            matched_old[old_index] = true;
        }
        matches.push(matched);
    }

    for (new_index, maybe_old_index) in matches.iter().enumerate() {
        if let Some(old_index) = maybe_old_index {
            let mut child_path = parent_path.to_vec();
            child_path.push(*old_index);
            reconcile_node(
                &old_children[*old_index],
                &new_children[new_index],
                &child_path,
                patches,
            );
        }
    }

    for old_index in (0..old_children.len()).rev() {
        if !matched_old[old_index] {
            patches.push(Patch::RemoveChild {
                parent_path: parent_path.to_vec(),
                index: old_index,
            });
        }
    }

    let mut current_order = matched_old
        .iter()
        .enumerate()
        .filter_map(|(index, matched)| matched.then_some(index))
        .collect::<Vec<_>>();

    for (new_index, maybe_old_index) in matches.iter().enumerate() {
        match maybe_old_index {
            Some(old_index) => {
                let Some(current_pos) = current_order.iter().position(|value| value == old_index)
                else {
                    continue;
                };
                if current_pos != new_index {
                    patches.push(Patch::MoveChild {
                        parent_path: parent_path.to_vec(),
                        from: current_pos,
                        to: new_index,
                    });
                    let moved = current_order.remove(current_pos);
                    current_order.insert(new_index, moved);
                }
            }
            None => {
                patches.push(Patch::InsertChild {
                    parent_path: parent_path.to_vec(),
                    index: new_index,
                    node: new_children[new_index].clone(),
                });
                current_order.insert(new_index, usize::MAX);
            }
        }
    }
}
