use std::rc::Rc;

use markup5ever_rcdom::{Node, NodeData};

pub(crate) fn get_node_tag_name(node: &Rc<Node>) -> Option<&str> {
    match &node.data {
        NodeData::Document => Some("html"),
        NodeData::Element { name, .. } => Some(&name.local),
        _ => None,
    }
}

pub(crate) fn get_parent_node(node: &Rc<Node>) -> Option<Rc<Node>> {
    let value = node.parent.take();
    let parent = value.as_ref()?;
    let Some(parent) = parent.upgrade() else {
        // Put the parent back
        node.parent.set(value);
        return None;
    };
    // Put the parent back
    node.parent.set(value);
    Some(parent)
}

pub(crate) fn get_node_children(node: &Rc<Node>) -> Vec<Rc<Node>> {
    let children = node.children.borrow();
    children.iter().cloned().collect()
}

pub(crate) fn get_node_content(node: &Rc<Node>) -> String {
    let mut content = String::new();
    
    for child in get_node_children(node) {
        match &child.data {
            NodeData::Text { contents } => {
                content.push_str(&contents.borrow());
            }
            NodeData::Element { .. } => {
                content.push_str(&get_node_content(&child));
            }
            _ => {}
        }
    }
    
    content
}
