#![allow(clippy::transmute_ptr_to_ref)]

use std::iter::FusedIterator;
use std::mem::transmute;
use std::ops::Index;

/// Capture Tree Node
///
/// Represents a single node in the capture tree. Can be queried for
/// information about the given capture and any child-captures that
/// took place.
#[derive(Debug)]
#[repr(transparent)]
pub struct CaptureTreeNode {
    raw: onig_sys::OnigCaptureTreeNode,
}

impl CaptureTreeNode {
    /// The capture group number for this capture
    pub fn group(&self) -> usize {
        self.raw.group as usize
    }

    /// The extent of this capture
    pub fn pos(&self) -> (usize, usize) {
        (self.raw.beg as usize, self.raw.end as usize)
    }

    /// The number of child captures this group contains
    pub fn len(&self) -> usize {
        self.raw.num_childs as usize
    }

    /// Does the node have any child captures?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// An iterator over thie children of this capture group
    pub fn children(&self) -> CaptureTreeNodeIter<'_> {
        CaptureTreeNodeIter { idx: 0, node: self }
    }
}

impl Index<usize> for CaptureTreeNode {
    type Output = CaptureTreeNode;

    fn index(&self, index: usize) -> &CaptureTreeNode {
        if index >= self.len() {
            panic!("capture tree node index overflow")
        }
        unsafe { transmute(*self.raw.childs.add(index)) }
    }
}

/// Captures iterator
#[derive(Debug)]
pub struct CaptureTreeNodeIter<'t> {
    idx: usize,
    node: &'t CaptureTreeNode,
}

impl<'t> Iterator for CaptureTreeNodeIter<'t> {
    type Item = &'t CaptureTreeNode;

    fn next(&mut self) -> Option<&'t CaptureTreeNode> {
        if self.idx < self.node.len() {
            self.idx += 1;
            Some(&self.node[self.idx - 1])
        } else {
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let size = self.node.len();
        (size, Some(size))
    }

    fn count(self) -> usize {
        self.node.len()
    }
}

impl<'t> FusedIterator for CaptureTreeNodeIter<'t> {}

impl<'t> ExactSizeIterator for CaptureTreeNodeIter<'t> {}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_regex_search_with_region_tree() {
        let mut region = Region::new();
        let mut syntax = Syntax::ruby().clone();
        syntax.enable_operators(SyntaxOperator::SYNTAX_OPERATOR_ATMARK_CAPTURE_HISTORY);

        let regex = Regex::with_options(
            "(?@a+(?@b+))|(?@c+(?@d+))",
            RegexOptions::REGEX_OPTION_NONE,
            &syntax,
        )
        .unwrap();

        let r = regex.search_with_options(
            "- cd aaabbb -",
            0,
            13,
            SearchOptions::SEARCH_OPTION_NONE,
            Some(&mut region),
        );

        assert_eq!(r, Some(2));
        assert_eq!(region.len(), 5);

        let tree = region.tree().unwrap();

        assert_eq!(tree.len(), 1);
        assert_eq!(tree.group(), 0);
        assert_eq!(tree.pos(), (2, 4));

        assert_eq!(tree[0].len(), 1);
        assert_eq!(tree[0].group(), 3);
        assert_eq!(tree[0].pos(), (2, 4));

        assert_eq!(tree[0][0].len(), 0);
        assert_eq!(tree[0][0].group(), 4);
        assert_eq!(tree[0][0].pos(), (3, 4));
    }
}
