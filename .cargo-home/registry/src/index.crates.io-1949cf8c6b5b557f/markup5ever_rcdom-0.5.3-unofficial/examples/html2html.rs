// Copyright 2014-2017 The html5ever Project Developers. See the
// COPYRIGHT file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! Parse and re-serialize a HTML5 document.
//!
//! This is meant to produce the exact same output (ignoring stderr) as
//!
//!   java -classpath htmlparser-1.4.jar nu.validator.htmlparser.tools.HTML2HTML
//!
//! where htmlparser-1.4.jar comes from http://about.validator.nu/htmlparser/

extern crate html5ever;
extern crate markup5ever_rcdom as rcdom;

use std::io::{self, Write};

use html5ever::driver::ParseOpts;
use html5ever::tendril::TendrilSink;
use html5ever::tree_builder::TreeBuilderOpts;
use html5ever::{parse_document, serialize};
use rcdom::{RcDom, SerializableHandle};

fn main() {
    env_logger::init();

    let opts = ParseOpts {
        tree_builder: TreeBuilderOpts {
            drop_doctype: true,
            ..Default::default()
        },
        ..Default::default()
    };
    let stdin = io::stdin();
    let dom = parse_document(RcDom::default(), opts)
        .from_utf8()
        .read_from(&mut stdin.lock())
        .unwrap();

    // The validator.nu HTML2HTML always prints a doctype at the very beginning.
    io::stdout()
        .write_all(b"<!DOCTYPE html>\n")
        .expect("writing DOCTYPE failed");
    let document: SerializableHandle = dom.document.clone().into();
    serialize(&mut io::stdout(), &document, Default::default()).expect("serialization failed");
}
