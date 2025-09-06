#!/usr/bin/env run-cargo-script
//! This is a regular crate doc comment, but it also contains a partial
//! Cargo manifest.  Note the use of a *fenced* code block, and the
//! `cargo` "language".
//!
//! ```cargo
//! [dependencies]
//! xml5ever = "0.2.0"
//! tendril = "0.1.3"
//! ```
extern crate markup5ever_rcdom as rcdom;
extern crate xml5ever;

use std::io;

use rcdom::{Handle, NodeData, RcDom};
use xml5ever::driver::parse_document;
use xml5ever::tendril::TendrilSink;

fn walk(prefix: &str, handle: &Handle) {
    let node = handle;

    print!("{prefix}");
    match node.data {
        NodeData::Document => println!("#document"),

        NodeData::Text { ref contents } => println!("#text {}", contents.borrow().escape_default()),

        NodeData::Element { ref name, .. } => {
            println!("{}", name.local);
        },

        _ => {},
    }

    let new_indent = {
        let mut temp = String::new();
        temp.push_str(prefix);
        temp.push_str("    ");
        temp
    };

    for child in node
        .children
        .borrow()
        .iter()
        .filter(|child| matches!(child.data, NodeData::Text { .. } | NodeData::Element { .. }))
    {
        walk(&new_indent, child);
    }
}

fn main() {
    env_logger::init();

    let stdin = io::stdin();

    // To parse XML into a tree form, we need a TreeSink
    // luckily xml5ever comes with a static RC backed tree represetation.
    let dom: RcDom = parse_document(RcDom::default(), Default::default())
        .from_utf8()
        .read_from(&mut stdin.lock())
        .unwrap();

    // Execute our visualizer on RcDom
    walk("", &dom.document);
}
