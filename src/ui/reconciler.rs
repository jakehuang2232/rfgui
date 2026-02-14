use crate::ui::{PropValue, RsxElementNode, RsxNode};

#[derive(Clone, Debug, PartialEq)]
pub enum Patch {
    ReplaceRoot(RsxNode),
    UpdateProps(Vec<(String, PropValue)>),
    ReplaceChildren(Vec<RsxNode>),
}

pub fn reconcile(old: Option<&RsxNode>, new: &RsxNode) -> Vec<Patch> {
    let Some(old) = old else {
        return vec![Patch::ReplaceRoot(new.clone())];
    };

    match (old, new) {
        (RsxNode::Element(old_node), RsxNode::Element(new_node)) => {
            reconcile_element(old_node, new_node)
        }
        (RsxNode::Text(a), RsxNode::Text(b)) => {
            if a == b {
                Vec::new()
            } else {
                vec![Patch::ReplaceRoot(new.clone())]
            }
        }
        (RsxNode::Fragment(a), RsxNode::Fragment(b)) => {
            if a == b {
                Vec::new()
            } else {
                vec![Patch::ReplaceChildren(b.clone())]
            }
        }
        _ => vec![Patch::ReplaceRoot(new.clone())],
    }
}

fn reconcile_element(old: &RsxElementNode, new: &RsxElementNode) -> Vec<Patch> {
    if old.tag != new.tag {
        return vec![Patch::ReplaceRoot(RsxNode::Element(new.clone()))];
    }

    let mut patches = Vec::new();
    if old.props != new.props {
        patches.push(Patch::UpdateProps(new.props.clone()));
    }
    if old.children != new.children {
        patches.push(Patch::ReplaceChildren(new.children.clone()));
    }
    patches
}
