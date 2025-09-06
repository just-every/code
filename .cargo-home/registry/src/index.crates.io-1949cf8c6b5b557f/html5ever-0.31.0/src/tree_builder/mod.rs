// Copyright 2014-2017 The html5ever Project Developers. See the
// COPYRIGHT file at the top-level directory of this distribution.
//
// Licensed under the Apache License, Version 2.0 <LICENSE-APACHE or
// http://www.apache.org/licenses/LICENSE-2.0> or the MIT license
// <LICENSE-MIT or http://opensource.org/licenses/MIT>, at your
// option. This file may not be copied, modified, or distributed
// except according to those terms.

//! The HTML5 tree builder.

pub use crate::interface::{create_element, ElemName, ElementFlags, Tracer, TreeSink};
pub use crate::interface::{AppendNode, AppendText, Attribute, NodeOrText};
pub use crate::interface::{LimitedQuirks, NoQuirks, Quirks, QuirksMode};

use self::types::*;

use crate::tendril::StrTendril;
use crate::{ExpandedName, LocalName, Namespace, QualName};

use crate::tokenizer;
use crate::tokenizer::states as tok_state;
use crate::tokenizer::{Doctype, EndTag, StartTag, Tag, TokenSink, TokenSinkResult};

use std::borrow::Cow::Borrowed;
use std::cell::{Cell, Ref, RefCell};
use std::collections::VecDeque;
use std::iter::{Enumerate, Rev};
use std::{fmt, slice};

use crate::tokenizer::states::RawKind;
use crate::tree_builder::tag_sets::*;
use crate::util::str::to_escaped_string;
use log::{debug, log_enabled, warn, Level};
use mac::format_if;
use markup5ever::{expanded_name, local_name, namespace_prefix, namespace_url, ns};

pub use self::PushFlag::*;

#[macro_use]
mod tag_sets;

mod data;
mod rules;
mod types;

/// Tree builder options, with an impl for Default.
#[derive(Copy, Clone)]
pub struct TreeBuilderOpts {
    /// Report all parse errors described in the spec, at some
    /// performance penalty?  Default: false
    pub exact_errors: bool,

    /// Is scripting enabled?
    pub scripting_enabled: bool,

    /// Is this an `iframe srcdoc` document?
    pub iframe_srcdoc: bool,

    /// Should we drop the DOCTYPE (if any) from the tree?
    pub drop_doctype: bool,

    /// Obsolete, ignored.
    pub ignore_missing_rules: bool,

    /// Initial TreeBuilder quirks mode. Default: NoQuirks
    pub quirks_mode: QuirksMode,
}

impl Default for TreeBuilderOpts {
    fn default() -> TreeBuilderOpts {
        TreeBuilderOpts {
            exact_errors: false,
            scripting_enabled: true,
            iframe_srcdoc: false,
            drop_doctype: false,
            ignore_missing_rules: false,
            quirks_mode: NoQuirks,
        }
    }
}

/// The HTML tree builder.
pub struct TreeBuilder<Handle, Sink> {
    /// Options controlling the behavior of the tree builder.
    opts: TreeBuilderOpts,

    /// Consumer of tree modifications.
    pub sink: Sink,

    /// Insertion mode.
    mode: Cell<InsertionMode>,

    /// Original insertion mode, used by Text and InTableText modes.
    orig_mode: Cell<Option<InsertionMode>>,

    /// Stack of template insertion modes.
    template_modes: RefCell<Vec<InsertionMode>>,

    /// Pending table character tokens.
    pending_table_text: RefCell<Vec<(SplitStatus, StrTendril)>>,

    /// Quirks mode as set by the parser.
    /// FIXME: can scripts etc. change this?
    quirks_mode: Cell<QuirksMode>,

    /// The document node, which is created by the sink.
    doc_handle: Handle,

    /// Stack of open elements, most recently added at end.
    open_elems: RefCell<Vec<Handle>>,

    /// List of active formatting elements.
    active_formatting: RefCell<Vec<FormatEntry<Handle>>>,

    //§ the-element-pointers
    /// Head element pointer.
    head_elem: RefCell<Option<Handle>>,

    /// Form element pointer.
    form_elem: RefCell<Option<Handle>>,
    //§ END
    /// Frameset-ok flag.
    frameset_ok: Cell<bool>,

    /// Ignore a following U+000A LINE FEED?
    ignore_lf: Cell<bool>,

    /// Is foster parenting enabled?
    foster_parenting: Cell<bool>,

    /// The context element for the fragment parsing algorithm.
    context_elem: RefCell<Option<Handle>>,

    /// Track current line
    current_line: Cell<u64>,
    // WARNING: If you add new fields that contain Handles, you
    // must add them to trace_handles() below to preserve memory
    // safety!
    //
    // FIXME: Auto-generate the trace hooks like Servo does.
}

impl<Handle, Sink> TreeBuilder<Handle, Sink>
where
    Handle: Clone,
    Sink: TreeSink<Handle = Handle>,
{
    /// Create a new tree builder which sends tree modifications to a particular `TreeSink`.
    ///
    /// The tree builder is also a `TokenSink`.
    pub fn new(sink: Sink, opts: TreeBuilderOpts) -> TreeBuilder<Handle, Sink> {
        let doc_handle = sink.get_document();
        TreeBuilder {
            opts,
            sink,
            mode: Cell::new(Initial),
            orig_mode: Cell::new(None),
            template_modes: Default::default(),
            pending_table_text: Default::default(),
            quirks_mode: Cell::new(opts.quirks_mode),
            doc_handle,
            open_elems: Default::default(),
            active_formatting: Default::default(),
            head_elem: Default::default(),
            form_elem: Default::default(),
            frameset_ok: Cell::new(true),
            ignore_lf: Default::default(),
            foster_parenting: Default::default(),
            context_elem: Default::default(),
            current_line: Cell::new(1),
        }
    }

    /// Create a new tree builder which sends tree modifications to a particular `TreeSink`.
    /// This is for parsing fragments.
    ///
    /// The tree builder is also a `TokenSink`.
    pub fn new_for_fragment(
        sink: Sink,
        context_elem: Handle,
        form_elem: Option<Handle>,
        opts: TreeBuilderOpts,
    ) -> TreeBuilder<Handle, Sink> {
        let doc_handle = sink.get_document();
        let context_is_template =
            sink.elem_name(&context_elem).expanded() == expanded_name!(html "template");
        let template_modes = if context_is_template {
            RefCell::new(vec![InTemplate])
        } else {
            RefCell::new(vec![])
        };

        let tb = TreeBuilder {
            opts,
            sink,
            mode: Cell::new(Initial),
            orig_mode: Cell::new(None),
            template_modes,
            pending_table_text: Default::default(),
            quirks_mode: Cell::new(opts.quirks_mode),
            doc_handle,
            open_elems: Default::default(),
            active_formatting: Default::default(),
            head_elem: Default::default(),
            form_elem: RefCell::new(form_elem),
            frameset_ok: Cell::new(true),
            ignore_lf: Default::default(),
            foster_parenting: Default::default(),
            context_elem: RefCell::new(Some(context_elem)),
            current_line: Cell::new(1),
        };

        // https://html.spec.whatwg.org/multipage/#parsing-html-fragments
        // 5. Let root be a new html element with no attributes.
        // 6. Append the element root to the Document node created above.
        // 7. Set up the parser's stack of open elements so that it contains just the single element root.
        tb.create_root(vec![]);
        // 10. Reset the parser's insertion mode appropriately.
        let old_insertion_mode = tb.reset_insertion_mode();
        tb.mode.set(old_insertion_mode);

        tb
    }

    // https://html.spec.whatwg.org/multipage/#concept-frag-parse-context
    // Step 4. Set the state of the HTML parser's tokenization stage as follows:
    pub fn tokenizer_state_for_context_elem(&self) -> tok_state::State {
        let context_elem = self.context_elem.borrow();
        let elem = context_elem.as_ref().expect("no context element");
        let elem_name = self.sink.elem_name(elem);
        let name = match elem_name.expanded() {
            ExpandedName {
                ns: &ns!(html),
                local,
            } => local,
            _ => return tok_state::Data,
        };
        match *name {
            local_name!("title") | local_name!("textarea") => tok_state::RawData(tok_state::Rcdata),

            local_name!("style")
            | local_name!("xmp")
            | local_name!("iframe")
            | local_name!("noembed")
            | local_name!("noframes") => tok_state::RawData(tok_state::Rawtext),

            local_name!("script") => tok_state::RawData(tok_state::ScriptData),

            local_name!("noscript") => {
                if self.opts.scripting_enabled {
                    tok_state::RawData(tok_state::Rawtext)
                } else {
                    tok_state::Data
                }
            },

            local_name!("plaintext") => tok_state::Plaintext,

            _ => tok_state::Data,
        }
    }

    /// Call the `Tracer`'s `trace_handle` method on every `Handle` in the tree builder's
    /// internal state.  This is intended to support garbage-collected DOMs.
    pub fn trace_handles(&self, tracer: &dyn Tracer<Handle = Handle>) {
        tracer.trace_handle(&self.doc_handle);
        for e in &*self.open_elems.borrow() {
            tracer.trace_handle(e);
        }

        for e in &*self.active_formatting.borrow() {
            if let FormatEntry::Element(handle, _) = e {
                tracer.trace_handle(handle);
            }
        }

        if let Some(head_elem) = self.head_elem.borrow().as_ref() {
            tracer.trace_handle(head_elem);
        }

        if let Some(form_elem) = self.form_elem.borrow().as_ref() {
            tracer.trace_handle(form_elem);
        }

        if let Some(context_elem) = self.context_elem.borrow().as_ref() {
            tracer.trace_handle(context_elem);
        }
    }

    #[allow(dead_code)]
    fn dump_state(&self, label: String) {
        println!("dump_state on {label}");
        print!("    open_elems:");
        for node in self.open_elems.borrow().iter() {
            let name = self.sink.elem_name(node);
            match *name.ns() {
                ns!(html) => print!(" {}", name.local_name()),
                _ => panic!(),
            }
        }
        println!();
        print!("    active_formatting:");
        for entry in self.active_formatting.borrow().iter() {
            match entry {
                &Marker => print!(" Marker"),
                Element(h, _) => {
                    let name = self.sink.elem_name(h);
                    match *name.ns() {
                        ns!(html) => print!(" {}", name.local_name()),
                        _ => panic!(),
                    }
                },
            }
        }
        println!();
    }

    fn debug_step(&self, mode: InsertionMode, token: &Token) {
        if log_enabled!(Level::Debug) {
            debug!(
                "processing {} in insertion mode {:?}",
                to_escaped_string(token),
                mode
            );
        }
    }

    fn process_to_completion(&self, mut token: Token) -> TokenSinkResult<Handle> {
        // Queue of additional tokens yet to be processed.
        // This stays empty in the common case where we don't split whitespace.
        let mut more_tokens = VecDeque::new();

        loop {
            let should_have_acknowledged_self_closing_flag = matches!(
                token,
                TagToken(Tag {
                    self_closing: true,
                    kind: StartTag,
                    ..
                })
            );
            let result = if self.is_foreign(&token) {
                self.step_foreign(token)
            } else {
                let mode = self.mode.get();
                self.step(mode, token)
            };
            match result {
                Done => {
                    if should_have_acknowledged_self_closing_flag {
                        self.sink
                            .parse_error(Borrowed("Unacknowledged self-closing tag"));
                    }
                    token = unwrap_or_return!(
                        more_tokens.pop_front(),
                        tokenizer::TokenSinkResult::Continue
                    );
                },
                DoneAckSelfClosing => {
                    token = unwrap_or_return!(
                        more_tokens.pop_front(),
                        tokenizer::TokenSinkResult::Continue
                    );
                },
                Reprocess(m, t) => {
                    self.mode.set(m);
                    token = t;
                },
                ReprocessForeign(t) => {
                    token = t;
                },
                SplitWhitespace(mut buf) => {
                    let p = buf.pop_front_char_run(|c| c.is_ascii_whitespace());
                    let (first, is_ws) = unwrap_or_return!(p, tokenizer::TokenSinkResult::Continue);
                    let status = if is_ws { Whitespace } else { NotWhitespace };
                    token = CharacterTokens(status, first);

                    if buf.len32() > 0 {
                        more_tokens.push_back(CharacterTokens(NotSplit, buf));
                    }
                },
                Script(node) => {
                    assert!(more_tokens.is_empty());
                    return tokenizer::TokenSinkResult::Script(node);
                },
                ToPlaintext => {
                    assert!(more_tokens.is_empty());
                    return tokenizer::TokenSinkResult::Plaintext;
                },
                ToRawData(k) => {
                    assert!(more_tokens.is_empty());
                    return tokenizer::TokenSinkResult::RawData(k);
                },
            }
        }
    }

    /// Are we parsing a HTML fragment?
    pub fn is_fragment(&self) -> bool {
        self.context_elem.borrow().is_some()
    }

    /// https://html.spec.whatwg.org/multipage/#appropriate-place-for-inserting-a-node
    fn appropriate_place_for_insertion(
        &self,
        override_target: Option<Handle>,
    ) -> InsertionPoint<Handle> {
        use self::tag_sets::*;

        declare_tag_set!(foster_target = "table" "tbody" "tfoot" "thead" "tr");
        let target = override_target.unwrap_or_else(|| self.current_node().clone());
        if !(self.foster_parenting.get() && self.elem_in(&target, foster_target)) {
            if self.html_elem_named(&target, local_name!("template")) {
                // No foster parenting (inside template).
                let contents = self.sink.get_template_contents(&target);
                return LastChild(contents);
            } else {
                // No foster parenting (the common case).
                return LastChild(target);
            }
        }

        // Foster parenting
        let open_elems = self.open_elems.borrow();
        let mut iter = open_elems.iter().rev().peekable();
        while let Some(elem) = iter.next() {
            if self.html_elem_named(elem, local_name!("template")) {
                let contents = self.sink.get_template_contents(elem);
                return LastChild(contents);
            } else if self.html_elem_named(elem, local_name!("table")) {
                return TableFosterParenting {
                    element: elem.clone(),
                    prev_element: (*iter.peek().unwrap()).clone(),
                };
            }
        }
        let html_elem = self.html_elem();
        LastChild(html_elem.clone())
    }

    fn insert_at(&self, insertion_point: InsertionPoint<Handle>, child: NodeOrText<Handle>) {
        match insertion_point {
            LastChild(parent) => self.sink.append(&parent, child),
            BeforeSibling(sibling) => self.sink.append_before_sibling(&sibling, child),
            TableFosterParenting {
                element,
                prev_element,
            } => self
                .sink
                .append_based_on_parent_node(&element, &prev_element, child),
        }
    }
}

impl<Handle, Sink> TokenSink for TreeBuilder<Handle, Sink>
where
    Handle: Clone,
    Sink: TreeSink<Handle = Handle>,
{
    type Handle = Handle;

    fn process_token(&self, token: tokenizer::Token, line_number: u64) -> TokenSinkResult<Handle> {
        if line_number != self.current_line.get() {
            self.sink.set_current_line(line_number);
        }
        let ignore_lf = self.ignore_lf.take();

        // Handle `ParseError` and `DoctypeToken`; convert everything else to the local `Token` type.
        let token = match token {
            tokenizer::ParseError(e) => {
                self.sink.parse_error(e);
                return tokenizer::TokenSinkResult::Continue;
            },

            tokenizer::DoctypeToken(dt) => {
                if self.mode.get() == Initial {
                    let (err, quirk) = data::doctype_error_and_quirks(&dt, self.opts.iframe_srcdoc);
                    if err {
                        self.sink.parse_error(format_if!(
                            self.opts.exact_errors,
                            "Bad DOCTYPE",
                            "Bad DOCTYPE: {:?}",
                            dt
                        ));
                    }
                    let Doctype {
                        name,
                        public_id,
                        system_id,
                        force_quirks: _,
                    } = dt;
                    if !self.opts.drop_doctype {
                        self.sink.append_doctype_to_document(
                            name.unwrap_or(StrTendril::new()),
                            public_id.unwrap_or(StrTendril::new()),
                            system_id.unwrap_or(StrTendril::new()),
                        );
                    }
                    self.set_quirks_mode(quirk);

                    self.mode.set(BeforeHtml);
                    return tokenizer::TokenSinkResult::Continue;
                } else {
                    self.sink.parse_error(format_if!(
                        self.opts.exact_errors,
                        "DOCTYPE in body",
                        "DOCTYPE in insertion mode {:?}",
                        self.mode.get()
                    ));
                    return tokenizer::TokenSinkResult::Continue;
                }
            },

            tokenizer::TagToken(x) => TagToken(x),
            tokenizer::CommentToken(x) => CommentToken(x),
            tokenizer::NullCharacterToken => NullCharacterToken,
            tokenizer::EOFToken => EOFToken,

            tokenizer::CharacterTokens(mut x) => {
                if ignore_lf && x.starts_with("\n") {
                    x.pop_front(1);
                }
                if x.is_empty() {
                    return tokenizer::TokenSinkResult::Continue;
                }
                CharacterTokens(NotSplit, x)
            },
        };

        self.process_to_completion(token)
    }

    fn end(&self) {
        for elem in self.open_elems.borrow_mut().drain(..).rev() {
            self.sink.pop(&elem);
        }
    }

    fn adjusted_current_node_present_but_not_in_html_namespace(&self) -> bool {
        !self.open_elems.borrow().is_empty()
            && *self.sink.elem_name(&self.adjusted_current_node()).ns() != ns!(html)
    }
}

pub fn html_elem<Handle>(open_elems: &[Handle]) -> &Handle {
    &open_elems[0]
}

struct ActiveFormattingView<'a, Handle: 'a> {
    data: Ref<'a, Vec<FormatEntry<Handle>>>,
}

impl<'a, Handle: 'a> ActiveFormattingView<'a, Handle> {
    fn iter(&'a self) -> impl Iterator<Item = (usize, &'a Handle, &'a Tag)> + 'a {
        ActiveFormattingIter {
            iter: self.data.iter().enumerate().rev(),
        }
    }
}

pub struct ActiveFormattingIter<'a, Handle: 'a> {
    iter: Rev<Enumerate<slice::Iter<'a, FormatEntry<Handle>>>>,
}

impl<'a, Handle> Iterator for ActiveFormattingIter<'a, Handle> {
    type Item = (usize, &'a Handle, &'a Tag);
    fn next(&mut self) -> Option<(usize, &'a Handle, &'a Tag)> {
        match self.iter.next() {
            None | Some((_, &Marker)) => None,
            Some((i, Element(h, t))) => Some((i, h, t)),
        }
    }
}

pub enum PushFlag {
    Push,
    NoPush,
}

enum Bookmark<Handle> {
    Replace(Handle),
    InsertAfter(Handle),
}

macro_rules! qualname {
    ("", $local:tt) => {
        QualName {
            prefix: None,
            ns: ns!(),
            local: local_name!($local),
        }
    };
    ($prefix: tt $ns:tt $local:tt) => {
        QualName {
            prefix: Some(namespace_prefix!($prefix)),
            ns: ns!($ns),
            local: local_name!($local),
        }
    };
}

#[doc(hidden)]
impl<Handle, Sink> TreeBuilder<Handle, Sink>
where
    Handle: Clone,
    Sink: TreeSink<Handle = Handle>,
{
    fn unexpected<T: fmt::Debug>(&self, _thing: &T) -> ProcessResult<Handle> {
        self.sink.parse_error(format_if!(
            self.opts.exact_errors,
            "Unexpected token",
            "Unexpected token {} in insertion mode {:?}",
            to_escaped_string(_thing),
            self.mode.get()
        ));
        Done
    }

    fn assert_named(&self, node: &Handle, name: LocalName) {
        assert!(self.html_elem_named(node, name));
    }

    /// Iterate over the active formatting elements (with index in the list) from the end
    /// to the last marker, or the beginning if there are no markers.
    fn active_formatting_end_to_marker(&self) -> ActiveFormattingView<'_, Handle> {
        ActiveFormattingView {
            data: self.active_formatting.borrow(),
        }
    }

    fn position_in_active_formatting(&self, element: &Handle) -> Option<usize> {
        self.active_formatting
            .borrow()
            .iter()
            .position(|n| match n {
                FormatEntry::Marker => false,
                FormatEntry::Element(ref handle, _) => self.sink.same_node(handle, element),
            })
    }

    fn set_quirks_mode(&self, mode: QuirksMode) {
        self.quirks_mode.set(mode);
        self.sink.set_quirks_mode(mode);
    }

    fn stop_parsing(&self) -> ProcessResult<Handle> {
        Done
    }

    //§ parsing-elements-that-contain-only-text
    // Switch to `Text` insertion mode, save the old mode, and
    // switch the tokenizer to a raw-data state.
    // The latter only takes effect after the current / next
    // `process_token` of a start tag returns!
    fn to_raw_text_mode(&self, k: RawKind) -> ProcessResult<Handle> {
        self.orig_mode.set(Some(self.mode.get()));
        self.mode.set(Text);
        ToRawData(k)
    }

    // The generic raw text / RCDATA parsing algorithm.
    fn parse_raw_data(&self, tag: Tag, k: RawKind) -> ProcessResult<Handle> {
        self.insert_element_for(tag);
        self.to_raw_text_mode(k)
    }
    //§ END

    fn current_node(&self) -> Ref<Handle> {
        Ref::map(self.open_elems.borrow(), |elems| {
            elems.last().expect("no current element")
        })
    }

    fn adjusted_current_node(&self) -> Ref<Handle> {
        if self.open_elems.borrow().len() == 1 {
            let context_elem = self.context_elem.borrow();
            let ctx = Ref::filter_map(context_elem, |e| e.as_ref());
            if let Ok(ctx) = ctx {
                return ctx;
            }
        }
        self.current_node()
    }

    fn current_node_in<TagSet>(&self, set: TagSet) -> bool
    where
        TagSet: Fn(ExpandedName) -> bool,
    {
        set(self.sink.elem_name(&self.current_node()).expanded())
    }

    // Insert at the "appropriate place for inserting a node".
    fn insert_appropriately(&self, child: NodeOrText<Handle>, override_target: Option<Handle>) {
        let insertion_point = self.appropriate_place_for_insertion(override_target);
        self.insert_at(insertion_point, child);
    }

    fn adoption_agency(&self, subject: LocalName) {
        // 1.
        if self.current_node_named(subject.clone())
            && self
                .position_in_active_formatting(&self.current_node())
                .is_none()
        {
            self.pop();
            return;
        }

        // 2. 3. 4.
        for _ in 0..8 {
            // 5.
            let (fmt_elem_index, fmt_elem, fmt_elem_tag) = unwrap_or_return!(
                // We clone the Handle and Tag so they don't cause an immutable borrow of self.
                self.active_formatting_end_to_marker()
                    .iter()
                    .find(|&(_, _, tag)| tag.name == subject)
                    .map(|(i, h, t)| (i, h.clone(), t.clone())),
                {
                    self.process_end_tag_in_body(Tag {
                        kind: EndTag,
                        name: subject,
                        self_closing: false,
                        attrs: vec![],
                    });
                }
            );

            let fmt_elem_stack_index = unwrap_or_return!(
                self.open_elems
                    .borrow()
                    .iter()
                    .rposition(|n| self.sink.same_node(n, &fmt_elem)),
                {
                    self.sink
                        .parse_error(Borrowed("Formatting element not open"));
                    self.active_formatting.borrow_mut().remove(fmt_elem_index);
                }
            );

            // 7.
            if !self.in_scope(default_scope, |n| self.sink.same_node(&n, &fmt_elem)) {
                self.sink
                    .parse_error(Borrowed("Formatting element not in scope"));
                return;
            }

            // 8.
            if !self.sink.same_node(&self.current_node(), &fmt_elem) {
                self.sink
                    .parse_error(Borrowed("Formatting element not current node"));
            }

            // 9.
            let (furthest_block_index, furthest_block) = unwrap_or_return!(
                self.open_elems
                    .borrow()
                    .iter()
                    .enumerate()
                    .skip(fmt_elem_stack_index)
                    .find(|&(_, open_element)| self.elem_in(open_element, special_tag))
                    .map(|(i, h)| (i, h.clone())),
                // 10.
                {
                    self.open_elems.borrow_mut().truncate(fmt_elem_stack_index);
                    self.active_formatting.borrow_mut().remove(fmt_elem_index);
                }
            );

            // 11.
            let common_ancestor = self.open_elems.borrow()[fmt_elem_stack_index - 1].clone();

            // 12.
            let mut bookmark = Bookmark::Replace(fmt_elem.clone());

            // 13.
            let mut node;
            let mut node_index = furthest_block_index;
            let mut last_node = furthest_block.clone();

            // 13.1.
            let mut inner_counter = 0;
            loop {
                // 13.2.
                inner_counter += 1;

                // 13.3.
                node_index -= 1;
                node = self.open_elems.borrow()[node_index].clone();

                // 13.4.
                if self.sink.same_node(&node, &fmt_elem) {
                    break;
                }

                // 13.5.
                if inner_counter > 3 {
                    self.position_in_active_formatting(&node)
                        .map(|position| self.active_formatting.borrow_mut().remove(position));
                    self.open_elems.borrow_mut().remove(node_index);
                    continue;
                }

                let node_formatting_index = unwrap_or_else!(
                    self.position_in_active_formatting(&node),
                    // 13.6.
                    {
                        self.open_elems.borrow_mut().remove(node_index);
                        continue;
                    }
                );

                // 13.7.
                let tag = match self.active_formatting.borrow()[node_formatting_index] {
                    Element(ref h, ref t) => {
                        assert!(self.sink.same_node(h, &node));
                        t.clone()
                    },
                    Marker => panic!("Found marker during adoption agency"),
                };
                // FIXME: Is there a way to avoid cloning the attributes twice here (once on their
                // own, once as part of t.clone() above)?
                let new_element = create_element(
                    &self.sink,
                    QualName::new(None, ns!(html), tag.name.clone()),
                    tag.attrs.clone(),
                );
                self.open_elems.borrow_mut()[node_index] = new_element.clone();
                self.active_formatting.borrow_mut()[node_formatting_index] =
                    Element(new_element.clone(), tag);
                node = new_element;

                // 13.8.
                if self.sink.same_node(&last_node, &furthest_block) {
                    bookmark = Bookmark::InsertAfter(node.clone());
                }

                // 13.9.
                self.sink.remove_from_parent(&last_node);
                self.sink.append(&node, AppendNode(last_node.clone()));

                // 13.10.
                last_node = node.clone();

                // 13.11.
            }

            // 14.
            self.sink.remove_from_parent(&last_node);
            self.insert_appropriately(AppendNode(last_node.clone()), Some(common_ancestor));

            // 15.
            // FIXME: Is there a way to avoid cloning the attributes twice here (once on their own,
            // once as part of t.clone() above)?
            let new_element = create_element(
                &self.sink,
                QualName::new(None, ns!(html), fmt_elem_tag.name.clone()),
                fmt_elem_tag.attrs.clone(),
            );
            let new_entry = Element(new_element.clone(), fmt_elem_tag);

            // 16.
            self.sink.reparent_children(&furthest_block, &new_element);

            // 17.
            self.sink
                .append(&furthest_block, AppendNode(new_element.clone()));

            // 18.
            // FIXME: We could probably get rid of the position_in_active_formatting() calls here
            // if we had a more clever Bookmark representation.
            match bookmark {
                Bookmark::Replace(to_replace) => {
                    let index = self
                        .position_in_active_formatting(&to_replace)
                        .expect("bookmark not found in active formatting elements");
                    self.active_formatting.borrow_mut()[index] = new_entry;
                },
                Bookmark::InsertAfter(previous) => {
                    let index = self
                        .position_in_active_formatting(&previous)
                        .expect("bookmark not found in active formatting elements")
                        + 1;
                    self.active_formatting.borrow_mut().insert(index, new_entry);
                    let old_index = self
                        .position_in_active_formatting(&fmt_elem)
                        .expect("formatting element not found in active formatting elements");
                    self.active_formatting.borrow_mut().remove(old_index);
                },
            }

            // 19.
            self.remove_from_stack(&fmt_elem);
            let new_furthest_block_index = self
                .open_elems
                .borrow()
                .iter()
                .position(|n| self.sink.same_node(n, &furthest_block))
                .expect("furthest block missing from open element stack");
            self.open_elems
                .borrow_mut()
                .insert(new_furthest_block_index + 1, new_element);

            // 20.
        }
    }

    fn push(&self, elem: &Handle) {
        self.open_elems.borrow_mut().push(elem.clone());
    }

    fn pop(&self) -> Handle {
        let elem = self
            .open_elems
            .borrow_mut()
            .pop()
            .expect("no current element");
        self.sink.pop(&elem);
        elem
    }

    fn remove_from_stack(&self, elem: &Handle) {
        let position = self
            .open_elems
            .borrow()
            .iter()
            .rposition(|x| self.sink.same_node(elem, x));
        if let Some(position) = position {
            self.open_elems.borrow_mut().remove(position);
            self.sink.pop(elem);
        }
    }

    fn is_marker_or_open(&self, entry: &FormatEntry<Handle>) -> bool {
        match *entry {
            Marker => true,
            Element(ref node, _) => self
                .open_elems
                .borrow()
                .iter()
                .rev()
                .any(|n| self.sink.same_node(n, node)),
        }
    }

    /// Reconstruct the active formatting elements.
    fn reconstruct_formatting(&self) {
        {
            let active_formatting = self.active_formatting.borrow();
            let last = unwrap_or_return!(active_formatting.last());
            if self.is_marker_or_open(last) {
                return;
            }
        }

        let mut entry_index = self.active_formatting.borrow().len() - 1;
        loop {
            if entry_index == 0 {
                break;
            }
            entry_index -= 1;
            if self.is_marker_or_open(&self.active_formatting.borrow()[entry_index]) {
                entry_index += 1;
                break;
            }
        }

        loop {
            let tag = match self.active_formatting.borrow()[entry_index] {
                Element(_, ref t) => t.clone(),
                Marker => panic!("Found marker during formatting element reconstruction"),
            };

            // FIXME: Is there a way to avoid cloning the attributes twice here (once on their own,
            // once as part of t.clone() above)?
            let new_element =
                self.insert_element(Push, ns!(html), tag.name.clone(), tag.attrs.clone());
            self.active_formatting.borrow_mut()[entry_index] = Element(new_element, tag);
            if entry_index == self.active_formatting.borrow().len() - 1 {
                break;
            }
            entry_index += 1;
        }
    }

    /// Get the first element on the stack, which will be the <html> element.
    fn html_elem(&self) -> Ref<Handle> {
        Ref::map(self.open_elems.borrow(), |elems| &elems[0])
    }

    /// Get the second element on the stack, if it's a HTML body element.
    fn body_elem(&self) -> Option<Ref<Handle>> {
        if self.open_elems.borrow().len() <= 1 {
            return None;
        }

        let node = Ref::map(self.open_elems.borrow(), |elems| &elems[1]);
        if self.html_elem_named(&node, local_name!("body")) {
            Some(node)
        } else {
            None
        }
    }

    /// Signal an error depending on the state of the stack of open elements at
    /// the end of the body.
    fn check_body_end(&self) {
        declare_tag_set!(body_end_ok =
            "dd" "dt" "li" "optgroup" "option" "p" "rp" "rt" "tbody" "td" "tfoot" "th"
            "thead" "tr" "body" "html");

        for elem in self.open_elems.borrow().iter() {
            let error;
            {
                let elem_name = self.sink.elem_name(elem);
                let name = elem_name.expanded();
                if body_end_ok(name) {
                    continue;
                }
                error = format_if!(
                    self.opts.exact_errors,
                    "Unexpected open tag at end of body",
                    "Unexpected open tag {:?} at end of body",
                    name
                );
            }
            self.sink.parse_error(error);
            // FIXME: Do we keep checking after finding one bad tag?
            // The spec suggests not.
            return;
        }
    }

    fn in_scope<TagSet, Pred>(&self, scope: TagSet, pred: Pred) -> bool
    where
        TagSet: Fn(ExpandedName) -> bool,
        Pred: Fn(Handle) -> bool,
    {
        for node in self.open_elems.borrow().iter().rev() {
            if pred(node.clone()) {
                return true;
            }
            if scope(self.sink.elem_name(node).expanded()) {
                return false;
            }
        }

        // supposed to be impossible, because <html> is always in scope

        false
    }

    fn elem_in<TagSet>(&self, elem: &Handle, set: TagSet) -> bool
    where
        TagSet: Fn(ExpandedName) -> bool,
    {
        set(self.sink.elem_name(elem).expanded())
    }

    fn html_elem_named(&self, elem: &Handle, name: LocalName) -> bool {
        let elem_name = self.sink.elem_name(elem);
        *elem_name.ns() == ns!(html) && *elem_name.local_name() == name
    }

    fn in_html_elem_named(&self, name: LocalName) -> bool {
        self.open_elems
            .borrow()
            .iter()
            .any(|elem| self.html_elem_named(elem, name.clone()))
    }

    fn current_node_named(&self, name: LocalName) -> bool {
        self.html_elem_named(&self.current_node(), name)
    }

    fn in_scope_named<TagSet>(&self, scope: TagSet, name: LocalName) -> bool
    where
        TagSet: Fn(ExpandedName) -> bool,
    {
        self.in_scope(scope, |elem| self.html_elem_named(&elem, name.clone()))
    }

    //§ closing-elements-that-have-implied-end-tags
    fn generate_implied_end<TagSet>(&self, set: TagSet)
    where
        TagSet: Fn(ExpandedName) -> bool,
    {
        loop {
            {
                let open_elems = self.open_elems.borrow();
                let elem = unwrap_or_return!(open_elems.last());
                let elem_name = self.sink.elem_name(elem);
                if !set(elem_name.expanded()) {
                    return;
                }
            }
            self.pop();
        }
    }

    fn generate_implied_end_except(&self, except: LocalName) {
        self.generate_implied_end(|p| {
            if *p.ns == ns!(html) && *p.local == except {
                false
            } else {
                cursory_implied_end(p)
            }
        });
    }
    //§ END

    // Pop elements until the current element is in the set.
    fn pop_until_current<TagSet>(&self, tag_set: TagSet)
    where
        TagSet: Fn(ExpandedName) -> bool,
    {
        while !self.current_node_in(&tag_set) {
            self.open_elems.borrow_mut().pop();
        }
    }

    // Pop elements until an element from the set has been popped.  Returns the
    // number of elements popped.
    fn pop_until<P>(&self, pred: P) -> usize
    where
        P: Fn(ExpandedName) -> bool,
    {
        let mut n = 0;
        loop {
            n += 1;
            match self.open_elems.borrow_mut().pop() {
                None => break,
                Some(elem) => {
                    if pred(self.sink.elem_name(&elem).expanded()) {
                        break;
                    }
                },
            }
        }
        n
    }

    fn pop_until_named(&self, name: LocalName) -> usize {
        self.pop_until(|p| *p.ns == ns!(html) && *p.local == name)
    }

    // Pop elements until one with the specified name has been popped.
    // Signal an error if it was not the first one.
    fn expect_to_close(&self, name: LocalName) {
        if self.pop_until_named(name.clone()) != 1 {
            self.sink.parse_error(format_if!(
                self.opts.exact_errors,
                "Unexpected open element",
                "Unexpected open element while closing {:?}",
                name
            ));
        }
    }

    fn close_p_element(&self) {
        declare_tag_set!(implied = [cursory_implied_end] - "p");
        self.generate_implied_end(implied);
        self.expect_to_close(local_name!("p"));
    }

    fn close_p_element_in_button_scope(&self) {
        if self.in_scope_named(button_scope, local_name!("p")) {
            self.close_p_element();
        }
    }

    // Check <input> tags for type=hidden
    fn is_type_hidden(&self, tag: &Tag) -> bool {
        match tag
            .attrs
            .iter()
            .find(|&at| at.name.expanded() == expanded_name!("", "type"))
        {
            None => false,
            Some(at) => at.value.eq_ignore_ascii_case("hidden"),
        }
    }

    fn foster_parent_in_body(&self, token: Token) -> ProcessResult<Handle> {
        warn!("foster parenting not implemented");
        self.foster_parenting.set(true);
        let res = self.step(InBody, token);
        // FIXME: what if res is Reprocess?
        self.foster_parenting.set(false);
        res
    }

    fn process_chars_in_table(&self, token: Token) -> ProcessResult<Handle> {
        declare_tag_set!(table_outer = "table" "tbody" "tfoot" "thead" "tr");
        if self.current_node_in(table_outer) {
            assert!(self.pending_table_text.borrow().is_empty());
            self.orig_mode.set(Some(self.mode.get()));
            Reprocess(InTableText, token)
        } else {
            self.sink.parse_error(format_if!(
                self.opts.exact_errors,
                "Unexpected characters in table",
                "Unexpected characters {} in table",
                to_escaped_string(&token)
            ));
            self.foster_parent_in_body(token)
        }
    }

    // https://html.spec.whatwg.org/multipage/#reset-the-insertion-mode-appropriately
    fn reset_insertion_mode(&self) -> InsertionMode {
        let open_elems = self.open_elems.borrow();
        for (i, mut node) in open_elems.iter().enumerate().rev() {
            let last = i == 0usize;
            let context_elem = self.context_elem.borrow();
            if let (true, Some(ctx)) = (last, context_elem.as_ref()) {
                node = ctx;
            }
            let elem_name = self.sink.elem_name(node);
            let name = match elem_name.expanded() {
                ExpandedName {
                    ns: &ns!(html),
                    local,
                } => local,
                _ => continue,
            };
            match *name {
                local_name!("select") => {
                    for ancestor in self.open_elems.borrow()[0..i].iter().rev() {
                        if self.html_elem_named(ancestor, local_name!("template")) {
                            return InSelect;
                        } else if self.html_elem_named(ancestor, local_name!("table")) {
                            return InSelectInTable;
                        }
                    }
                    return InSelect;
                },
                local_name!("td") | local_name!("th") => {
                    if !last {
                        return InCell;
                    }
                },
                local_name!("tr") => return InRow,
                local_name!("tbody") | local_name!("thead") | local_name!("tfoot") => {
                    return InTableBody;
                },
                local_name!("caption") => return InCaption,
                local_name!("colgroup") => return InColumnGroup,
                local_name!("table") => return InTable,
                local_name!("template") => return *self.template_modes.borrow().last().unwrap(),
                local_name!("head") => {
                    if !last {
                        return InHead;
                    }
                },
                local_name!("body") => return InBody,
                local_name!("frameset") => return InFrameset,
                local_name!("html") => match *self.head_elem.borrow() {
                    None => return BeforeHead,
                    Some(_) => return AfterHead,
                },

                _ => (),
            }
        }
        InBody
    }

    fn close_the_cell(&self) {
        self.generate_implied_end(cursory_implied_end);
        if self.pop_until(td_th) != 1 {
            self.sink
                .parse_error(Borrowed("expected to close <td> or <th> with cell"));
        }
        self.clear_active_formatting_to_marker();
    }

    fn append_text(&self, text: StrTendril) -> ProcessResult<Handle> {
        self.insert_appropriately(AppendText(text), None);
        Done
    }

    fn append_comment(&self, text: StrTendril) -> ProcessResult<Handle> {
        let comment = self.sink.create_comment(text);
        self.insert_appropriately(AppendNode(comment), None);
        Done
    }

    fn append_comment_to_doc(&self, text: StrTendril) -> ProcessResult<Handle> {
        let comment = self.sink.create_comment(text);
        self.sink.append(&self.doc_handle, AppendNode(comment));
        Done
    }

    fn append_comment_to_html(&self, text: StrTendril) -> ProcessResult<Handle> {
        let open_elems = self.open_elems.borrow();
        let target = html_elem(&open_elems);
        let comment = self.sink.create_comment(text);
        self.sink.append(target, AppendNode(comment));
        Done
    }

    //§ creating-and-inserting-nodes
    fn create_root(&self, attrs: Vec<Attribute>) {
        let elem = create_element(
            &self.sink,
            QualName::new(None, ns!(html), local_name!("html")),
            attrs,
        );
        self.push(&elem);
        self.sink.append(&self.doc_handle, AppendNode(elem));
        // FIXME: application cache selection algorithm
    }

    // https://html.spec.whatwg.org/multipage/#create-an-element-for-the-token
    fn insert_element(
        &self,
        push: PushFlag,
        ns: Namespace,
        name: LocalName,
        attrs: Vec<Attribute>,
    ) -> Handle {
        declare_tag_set!(form_associatable =
            "button" "fieldset" "input" "object"
            "output" "select" "textarea" "img");

        declare_tag_set!(listed = [form_associatable] - "img");

        // Step 7.
        let qname = QualName::new(None, ns, name);
        let elem = create_element(&self.sink, qname.clone(), attrs.clone());

        let insertion_point = self.appropriate_place_for_insertion(None);
        let (node1, node2) = match insertion_point {
            LastChild(ref p) | BeforeSibling(ref p) => (p.clone(), None),
            TableFosterParenting {
                ref element,
                ref prev_element,
            } => (element.clone(), Some(prev_element.clone())),
        };

        // Step 12.
        if form_associatable(qname.expanded())
            && self.form_elem.borrow().is_some()
            && !self.in_html_elem_named(local_name!("template"))
            && !(listed(qname.expanded())
                && attrs
                    .iter()
                    .any(|a| a.name.expanded() == expanded_name!("", "form")))
        {
            let form = self.form_elem.borrow().as_ref().unwrap().clone();
            self.sink
                .associate_with_form(&elem, &form, (&node1, node2.as_ref()));
        }

        self.insert_at(insertion_point, AppendNode(elem.clone()));

        match push {
            Push => self.push(&elem),
            NoPush => (),
        }
        // FIXME: Remove from the stack if we can't append?
        elem
    }

    fn insert_element_for(&self, tag: Tag) -> Handle {
        self.insert_element(Push, ns!(html), tag.name, tag.attrs)
    }

    fn insert_and_pop_element_for(&self, tag: Tag) -> Handle {
        self.insert_element(NoPush, ns!(html), tag.name, tag.attrs)
    }

    fn insert_phantom(&self, name: LocalName) -> Handle {
        self.insert_element(Push, ns!(html), name, vec![])
    }

    /// <https://html.spec.whatwg.org/multipage/parsing.html#insert-an-element-at-the-adjusted-insertion-location>
    fn insert_foreign_element(
        &self,
        tag: Tag,
        ns: Namespace,
        only_add_to_element_stack: bool,
    ) -> Handle {
        let adjusted_insertion_location = self.appropriate_place_for_insertion(None);
        let qname = QualName::new(None, ns, tag.name);
        let elem = create_element(&self.sink, qname.clone(), tag.attrs.clone());

        if !only_add_to_element_stack {
            self.insert_at(adjusted_insertion_location, AppendNode(elem.clone()));
        }

        self.push(&elem);

        elem
    }
    //§ END

    /// <https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inhead>
    ///
    /// A start tag whose tag name is "template"
    fn should_attach_declarative_shadow(&self, tag: &Tag) -> bool {
        let adjusted_insertion_location = self.appropriate_place_for_insertion(None);

        let (intended_parent, _node2) = match adjusted_insertion_location {
            LastChild(ref p) | BeforeSibling(ref p) => (p.clone(), None),
            TableFosterParenting {
                ref element,
                ref prev_element,
            } => (element.clone(), Some(prev_element.clone())),
        };

        // template start tag's shadowrootmode is not in the none state
        let is_shadow_root_mode = tag.attrs.iter().any(|attr| {
            attr.name.local == local_name!("shadowrootmode")
                && (attr.value.as_ref() == "open" || attr.value.as_ref() == "closed")
        });

        // Check if intended_parent's document allows declarative shadow roots
        let allow_declarative_shadow_roots =
            self.sink.allow_declarative_shadow_roots(&intended_parent);

        // the adjusted current node is not the topmost element in the stack of open elements
        let adjusted_current_node_not_topmost = match self.open_elems.borrow().first() {
            // The stack grows downwards; the topmost node on the stack is the first one added to the stack
            // The current node is the bottommost node in this stack of open elements.
            //
            // (1) The adjusted current node is the context element if the parser was created as part of the HTML fragment parsing algorithm
            // and the stack of open elements has only one element in it (fragment case);
            // (2) otherwise, the adjusted current node is the current node (the bottomost node)
            //
            // => adjusted current node != topmost element in the stack when the stack size > 1
            Some(_) => self.open_elems.borrow().len() > 1,
            None => true,
        };

        is_shadow_root_mode && allow_declarative_shadow_roots && adjusted_current_node_not_topmost
    }

    /// <https://html.spec.whatwg.org/multipage/parsing.html#parsing-main-inhead>
    ///
    /// A start tag whose tag name is "template"
    fn attach_declarative_shadow(
        &self,
        tag: &Tag,
        shadow_host: &Handle,
        template: &Handle,
    ) -> Result<(), String> {
        self.sink
            .attach_declarative_shadow(shadow_host, template, tag.attrs.clone())
    }

    fn create_formatting_element_for(&self, tag: Tag) -> Handle {
        // FIXME: This really wants unit tests.
        let mut first_match = None;
        let mut matches = 0usize;
        for (i, _, old_tag) in self.active_formatting_end_to_marker().iter() {
            if tag.equiv_modulo_attr_order(old_tag) {
                first_match = Some(i);
                matches += 1;
            }
        }

        if matches >= 3 {
            self.active_formatting
                .borrow_mut()
                .remove(first_match.expect("matches with no index"));
        }

        let elem = self.insert_element(Push, ns!(html), tag.name.clone(), tag.attrs.clone());
        self.active_formatting
            .borrow_mut()
            .push(Element(elem.clone(), tag));
        elem
    }

    fn clear_active_formatting_to_marker(&self) {
        loop {
            match self.active_formatting.borrow_mut().pop() {
                None | Some(Marker) => break,
                _ => (),
            }
        }
    }

    fn process_end_tag_in_body(&self, tag: Tag) {
        // Look back for a matching open element.
        let mut match_idx = None;
        for (i, elem) in self.open_elems.borrow().iter().enumerate().rev() {
            if self.html_elem_named(elem, tag.name.clone()) {
                match_idx = Some(i);
                break;
            }

            if self.elem_in(elem, special_tag) {
                self.sink
                    .parse_error(Borrowed("Found special tag while closing generic tag"));
                return;
            }
        }

        // Can't use unwrap_or_return!() due to rust-lang/rust#16617.
        let match_idx = match match_idx {
            None => {
                // I believe this is impossible, because the root
                // <html> element is in special_tag.
                self.unexpected(&tag);
                return;
            },
            Some(x) => x,
        };

        self.generate_implied_end_except(tag.name.clone());

        if match_idx != self.open_elems.borrow().len() - 1 {
            // mis-nested tags
            self.unexpected(&tag);
        }
        self.open_elems.borrow_mut().truncate(match_idx);
    }

    fn handle_misnested_a_tags(&self, tag: &Tag) {
        let node = unwrap_or_return!(self
            .active_formatting_end_to_marker()
            .iter()
            .find(|&(_, n, _)| self.html_elem_named(n, local_name!("a")))
            .map(|(_, n, _)| n.clone()));

        self.unexpected(tag);
        self.adoption_agency(local_name!("a"));
        self.position_in_active_formatting(&node)
            .map(|index| self.active_formatting.borrow_mut().remove(index));
        self.remove_from_stack(&node);
    }

    //§ tree-construction
    fn is_foreign(&self, token: &Token) -> bool {
        if let EOFToken = *token {
            return false;
        }

        if self.open_elems.borrow().is_empty() {
            return false;
        }

        let current = self.adjusted_current_node();
        let elem_name = self.sink.elem_name(&current);
        let name = elem_name.expanded();
        if let ns!(html) = *name.ns {
            return false;
        }

        if mathml_text_integration_point(name) {
            match *token {
                CharacterTokens(..) | NullCharacterToken => return false,
                TagToken(Tag {
                    kind: StartTag,
                    ref name,
                    ..
                }) if !matches!(*name, local_name!("mglyph") | local_name!("malignmark")) => {
                    return false;
                },
                _ => (),
            }
        }

        if svg_html_integration_point(name) {
            match *token {
                CharacterTokens(..) | NullCharacterToken => return false,
                TagToken(Tag { kind: StartTag, .. }) => return false,
                _ => (),
            }
        }

        if let expanded_name!(mathml "annotation-xml") = name {
            match *token {
                TagToken(Tag {
                    kind: StartTag,
                    name: local_name!("svg"),
                    ..
                }) => return false,
                CharacterTokens(..) | NullCharacterToken | TagToken(Tag { kind: StartTag, .. }) => {
                    return !self
                        .sink
                        .is_mathml_annotation_xml_integration_point(&self.adjusted_current_node());
                },
                _ => {},
            };
        }

        true
    }
    //§ END

    fn enter_foreign(&self, mut tag: Tag, ns: Namespace) -> ProcessResult<Handle> {
        match ns {
            ns!(mathml) => self.adjust_mathml_attributes(&mut tag),
            ns!(svg) => self.adjust_svg_attributes(&mut tag),
            _ => (),
        }
        self.adjust_foreign_attributes(&mut tag);

        if tag.self_closing {
            self.insert_element(NoPush, ns, tag.name, tag.attrs);
            DoneAckSelfClosing
        } else {
            self.insert_element(Push, ns, tag.name, tag.attrs);
            Done
        }
    }

    fn adjust_svg_tag_name(&self, tag: &mut Tag) {
        let Tag { ref mut name, .. } = *tag;
        match *name {
            local_name!("altglyph") => *name = local_name!("altGlyph"),
            local_name!("altglyphdef") => *name = local_name!("altGlyphDef"),
            local_name!("altglyphitem") => *name = local_name!("altGlyphItem"),
            local_name!("animatecolor") => *name = local_name!("animateColor"),
            local_name!("animatemotion") => *name = local_name!("animateMotion"),
            local_name!("animatetransform") => *name = local_name!("animateTransform"),
            local_name!("clippath") => *name = local_name!("clipPath"),
            local_name!("feblend") => *name = local_name!("feBlend"),
            local_name!("fecolormatrix") => *name = local_name!("feColorMatrix"),
            local_name!("fecomponenttransfer") => *name = local_name!("feComponentTransfer"),
            local_name!("fecomposite") => *name = local_name!("feComposite"),
            local_name!("feconvolvematrix") => *name = local_name!("feConvolveMatrix"),
            local_name!("fediffuselighting") => *name = local_name!("feDiffuseLighting"),
            local_name!("fedisplacementmap") => *name = local_name!("feDisplacementMap"),
            local_name!("fedistantlight") => *name = local_name!("feDistantLight"),
            local_name!("fedropshadow") => *name = local_name!("feDropShadow"),
            local_name!("feflood") => *name = local_name!("feFlood"),
            local_name!("fefunca") => *name = local_name!("feFuncA"),
            local_name!("fefuncb") => *name = local_name!("feFuncB"),
            local_name!("fefuncg") => *name = local_name!("feFuncG"),
            local_name!("fefuncr") => *name = local_name!("feFuncR"),
            local_name!("fegaussianblur") => *name = local_name!("feGaussianBlur"),
            local_name!("feimage") => *name = local_name!("feImage"),
            local_name!("femerge") => *name = local_name!("feMerge"),
            local_name!("femergenode") => *name = local_name!("feMergeNode"),
            local_name!("femorphology") => *name = local_name!("feMorphology"),
            local_name!("feoffset") => *name = local_name!("feOffset"),
            local_name!("fepointlight") => *name = local_name!("fePointLight"),
            local_name!("fespecularlighting") => *name = local_name!("feSpecularLighting"),
            local_name!("fespotlight") => *name = local_name!("feSpotLight"),
            local_name!("fetile") => *name = local_name!("feTile"),
            local_name!("feturbulence") => *name = local_name!("feTurbulence"),
            local_name!("foreignobject") => *name = local_name!("foreignObject"),
            local_name!("glyphref") => *name = local_name!("glyphRef"),
            local_name!("lineargradient") => *name = local_name!("linearGradient"),
            local_name!("radialgradient") => *name = local_name!("radialGradient"),
            local_name!("textpath") => *name = local_name!("textPath"),
            _ => (),
        }
    }

    fn adjust_attributes<F>(&self, tag: &mut Tag, mut map: F)
    where
        F: FnMut(LocalName) -> Option<QualName>,
    {
        for &mut Attribute { ref mut name, .. } in &mut tag.attrs {
            if let Some(replacement) = map(name.local.clone()) {
                *name = replacement;
            }
        }
    }

    fn adjust_svg_attributes(&self, tag: &mut Tag) {
        self.adjust_attributes(tag, |k| match k {
            local_name!("attributename") => Some(qualname!("", "attributeName")),
            local_name!("attributetype") => Some(qualname!("", "attributeType")),
            local_name!("basefrequency") => Some(qualname!("", "baseFrequency")),
            local_name!("baseprofile") => Some(qualname!("", "baseProfile")),
            local_name!("calcmode") => Some(qualname!("", "calcMode")),
            local_name!("clippathunits") => Some(qualname!("", "clipPathUnits")),
            local_name!("diffuseconstant") => Some(qualname!("", "diffuseConstant")),
            local_name!("edgemode") => Some(qualname!("", "edgeMode")),
            local_name!("filterunits") => Some(qualname!("", "filterUnits")),
            local_name!("glyphref") => Some(qualname!("", "glyphRef")),
            local_name!("gradienttransform") => Some(qualname!("", "gradientTransform")),
            local_name!("gradientunits") => Some(qualname!("", "gradientUnits")),
            local_name!("kernelmatrix") => Some(qualname!("", "kernelMatrix")),
            local_name!("kernelunitlength") => Some(qualname!("", "kernelUnitLength")),
            local_name!("keypoints") => Some(qualname!("", "keyPoints")),
            local_name!("keysplines") => Some(qualname!("", "keySplines")),
            local_name!("keytimes") => Some(qualname!("", "keyTimes")),
            local_name!("lengthadjust") => Some(qualname!("", "lengthAdjust")),
            local_name!("limitingconeangle") => Some(qualname!("", "limitingConeAngle")),
            local_name!("markerheight") => Some(qualname!("", "markerHeight")),
            local_name!("markerunits") => Some(qualname!("", "markerUnits")),
            local_name!("markerwidth") => Some(qualname!("", "markerWidth")),
            local_name!("maskcontentunits") => Some(qualname!("", "maskContentUnits")),
            local_name!("maskunits") => Some(qualname!("", "maskUnits")),
            local_name!("numoctaves") => Some(qualname!("", "numOctaves")),
            local_name!("pathlength") => Some(qualname!("", "pathLength")),
            local_name!("patterncontentunits") => Some(qualname!("", "patternContentUnits")),
            local_name!("patterntransform") => Some(qualname!("", "patternTransform")),
            local_name!("patternunits") => Some(qualname!("", "patternUnits")),
            local_name!("pointsatx") => Some(qualname!("", "pointsAtX")),
            local_name!("pointsaty") => Some(qualname!("", "pointsAtY")),
            local_name!("pointsatz") => Some(qualname!("", "pointsAtZ")),
            local_name!("preservealpha") => Some(qualname!("", "preserveAlpha")),
            local_name!("preserveaspectratio") => Some(qualname!("", "preserveAspectRatio")),
            local_name!("primitiveunits") => Some(qualname!("", "primitiveUnits")),
            local_name!("refx") => Some(qualname!("", "refX")),
            local_name!("refy") => Some(qualname!("", "refY")),
            local_name!("repeatcount") => Some(qualname!("", "repeatCount")),
            local_name!("repeatdur") => Some(qualname!("", "repeatDur")),
            local_name!("requiredextensions") => Some(qualname!("", "requiredExtensions")),
            local_name!("requiredfeatures") => Some(qualname!("", "requiredFeatures")),
            local_name!("specularconstant") => Some(qualname!("", "specularConstant")),
            local_name!("specularexponent") => Some(qualname!("", "specularExponent")),
            local_name!("spreadmethod") => Some(qualname!("", "spreadMethod")),
            local_name!("startoffset") => Some(qualname!("", "startOffset")),
            local_name!("stddeviation") => Some(qualname!("", "stdDeviation")),
            local_name!("stitchtiles") => Some(qualname!("", "stitchTiles")),
            local_name!("surfacescale") => Some(qualname!("", "surfaceScale")),
            local_name!("systemlanguage") => Some(qualname!("", "systemLanguage")),
            local_name!("tablevalues") => Some(qualname!("", "tableValues")),
            local_name!("targetx") => Some(qualname!("", "targetX")),
            local_name!("targety") => Some(qualname!("", "targetY")),
            local_name!("textlength") => Some(qualname!("", "textLength")),
            local_name!("viewbox") => Some(qualname!("", "viewBox")),
            local_name!("viewtarget") => Some(qualname!("", "viewTarget")),
            local_name!("xchannelselector") => Some(qualname!("", "xChannelSelector")),
            local_name!("ychannelselector") => Some(qualname!("", "yChannelSelector")),
            local_name!("zoomandpan") => Some(qualname!("", "zoomAndPan")),
            _ => None,
        });
    }

    fn adjust_mathml_attributes(&self, tag: &mut Tag) {
        self.adjust_attributes(tag, |k| match k {
            local_name!("definitionurl") => Some(qualname!("", "definitionURL")),
            _ => None,
        });
    }

    fn adjust_foreign_attributes(&self, tag: &mut Tag) {
        self.adjust_attributes(tag, |k| match k {
            local_name!("xlink:actuate") => Some(qualname!("xlink" xlink "actuate")),
            local_name!("xlink:arcrole") => Some(qualname!("xlink" xlink "arcrole")),
            local_name!("xlink:href") => Some(qualname!("xlink" xlink "href")),
            local_name!("xlink:role") => Some(qualname!("xlink" xlink "role")),
            local_name!("xlink:show") => Some(qualname!("xlink" xlink "show")),
            local_name!("xlink:title") => Some(qualname!("xlink" xlink "title")),
            local_name!("xlink:type") => Some(qualname!("xlink" xlink "type")),
            local_name!("xml:lang") => Some(qualname!("xml" xml "lang")),
            local_name!("xml:space") => Some(qualname!("xml" xml "space")),
            local_name!("xmlns") => Some(qualname!("" xmlns "xmlns")),
            local_name!("xmlns:xlink") => Some(qualname!("xmlns" xmlns "xlink")),
            _ => None,
        });
    }

    fn foreign_start_tag(&self, mut tag: Tag) -> ProcessResult<Handle> {
        let current_ns = self
            .sink
            .elem_name(&self.adjusted_current_node())
            .ns()
            .clone();
        match current_ns {
            ns!(mathml) => self.adjust_mathml_attributes(&mut tag),
            ns!(svg) => {
                self.adjust_svg_tag_name(&mut tag);
                self.adjust_svg_attributes(&mut tag);
            },
            _ => (),
        }
        self.adjust_foreign_attributes(&mut tag);
        if tag.self_closing {
            // FIXME(#118): <script /> in SVG
            self.insert_element(NoPush, current_ns, tag.name, tag.attrs);
            DoneAckSelfClosing
        } else {
            self.insert_element(Push, current_ns, tag.name, tag.attrs);
            Done
        }
    }

    fn unexpected_start_tag_in_foreign_content(&self, tag: Tag) -> ProcessResult<Handle> {
        self.unexpected(&tag);
        while !self.current_node_in(|n| {
            *n.ns == ns!(html) || mathml_text_integration_point(n) || svg_html_integration_point(n)
        }) {
            self.pop();
        }
        self.step(self.mode.get(), TagToken(tag))
    }
}
