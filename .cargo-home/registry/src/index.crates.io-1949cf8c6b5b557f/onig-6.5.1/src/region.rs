#![allow(clippy::transmute_ptr_to_ref)]

use std::iter::FusedIterator;
use std::mem::transmute;
use std::os::raw::{c_int, c_void};
use std::ptr::null_mut;

use super::flags::TraverseCallbackAt;
use super::CaptureTreeNode;

/// Represents a set of capture groups found in a search or match.
#[derive(Debug, Eq, PartialEq)]
#[repr(transparent)]
pub struct Region {
    pub(crate) raw: onig_sys::OnigRegion,
}

impl Region {
    /// Create a new empty `Region`
    pub fn new() -> Region {
        Region {
            raw: onig_sys::OnigRegion {
                allocated: 0,
                num_regs: 0,
                beg: null_mut(),
                end: null_mut(),
                history_root: null_mut(),
            },
        }
    }

    /// Create a new region with a given capacity. This function allocates
    /// a new region object as in `Region::new` and resizes it to
    /// contain at least `capacity` regions.
    ///
    /// # Arguments
    ///
    /// * `capacity` - the number of captures this region should be
    /// capable of storing without allocation.
    pub fn with_capacity(capacity: usize) -> Region {
        let mut region = Self::new();
        region.reserve(capacity);
        region
    }

    /// Clone From Raw
    ///
    /// Construct a new region based on an existing raw
    /// `*onig_sys::OnigRegion` pointer by copying.
    pub unsafe fn clone_from_raw(ptr: *mut onig_sys::OnigRegion) -> Self {
        let mut region = Self::new();
        onig_sys::onig_region_copy(&mut region.raw, ptr);
        region
    }

    /// This can be used to clear out a region so it can be used
    /// again. See [`onig_sys::onig_region_clear`][region_clear]
    ///
    /// [region_clear]: ./onig_sys/fn.onig_region_clear.html
    pub fn clear(&mut self) {
        unsafe {
            onig_sys::onig_region_clear(&mut self.raw);
        }
    }

    /// Get the current capacity of the region.
    pub fn capacity(&self) -> usize {
        self.raw.allocated as usize
    }

    /// Updates the region to contain `new_capacity` slots. See
    /// [`onig_sys::onig_region_resize`][region_resize] for mor
    /// information.
    ///
    /// [region_resize]: ./onig_sys/fn.onig_region_resize.html
    ///
    /// # Arguments
    ///
    ///  * `new_capacity` - The new number of groups in the region.
    pub fn reserve(&mut self, new_capacity: usize) {
        let r = unsafe { onig_sys::onig_region_resize(&mut self.raw, new_capacity as c_int) };
        if r != onig_sys::ONIG_NORMAL as i32 {
            panic!("Onig: fail to memory allocation during region resize")
        }
    }

    /// Get the size of the region.
    ///
    /// Returns the number of registers in the region.
    pub fn len(&self) -> usize {
        self.raw.num_regs as usize
    }

    /// Check if the region is empty.
    ///
    /// Returns true if there are no registers in the region.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the start and end positions of the Nth capture group.
    ///
    /// Returns `None` if `pos` is not a valid capture group or if the
    /// capture group did not match anything. The positions returned
    /// are always byte indices with respect to the original string
    /// matched.
    pub fn pos(&self, pos: usize) -> Option<(usize, usize)> {
        if pos >= self.len() {
            return None;
        }
        let pos = pos as isize;
        let (beg, end) = unsafe { (*self.raw.beg.offset(pos), *self.raw.end.offset(pos)) };
        if beg != onig_sys::ONIG_REGION_NOTPOS {
            Some((beg as usize, end as usize))
        } else {
            None
        }
    }

    /// Get Capture Tree
    ///
    /// Returns the capture tree for this region, if there is one.
    pub fn tree(&self) -> Option<&CaptureTreeNode> {
        let tree = unsafe { onig_sys::onig_get_capture_tree(self.raw_mut()) };
        if tree.is_null() {
            None
        } else {
            Some(unsafe { transmute(tree) })
        }
    }

    /// Get an iterator over the captures in the region.
    pub fn iter(&self) -> RegionIter<'_> {
        RegionIter {
            region: self,
            pos: 0,
        }
    }

    /// Walk the Tree of Captures
    ///
    /// The given callback is invoked for each node in the capture
    /// tree. Each node is passed to the callback before any children.
    pub fn tree_traverse<F>(&self, callback: F) -> i32
    where
        F: Fn(u32, (usize, usize), u32) -> bool,
    {
        self.tree_traverse_at(TraverseCallbackAt::CALLBACK_AT_FIRST, callback)
    }

    /// Walk the Tree of Captures in a Given Order
    ///
    /// The given callback is invoked for each node in the capture
    /// tree. The order in which the callback is invoked can be
    /// chosen.
    pub fn tree_traverse_at<F>(&self, at: TraverseCallbackAt, mut callback: F) -> i32
    where
        F: Fn(u32, (usize, usize), u32) -> bool,
    {
        use onig_sys::onig_capture_tree_traverse;

        extern "C" fn traverse_cb<F>(
            group: c_int,
            beg: c_int,
            end: c_int,
            level: c_int,
            _at: c_int,
            ud: *mut c_void,
        ) -> c_int
        where
            F: Fn(u32, (usize, usize), u32) -> bool,
        {
            let callback = unsafe { &*(ud as *mut F) };
            if callback(group as u32, (beg as usize, end as usize), level as u32) {
                0
            } else {
                -1
            }
        }

        unsafe {
            onig_capture_tree_traverse(
                self.raw_mut(),
                at.bits() as c_int,
                Some(traverse_cb::<F>),
                &mut callback as *mut F as *mut c_void,
            )
        }
    }

    /// Convert a reference to self to a mutable pointer. This
    /// shouldn't ever actually be used to mutate the underlying
    /// region. It's needed to match the bindgened types though.
    fn raw_mut(&self) -> *mut onig_sys::OnigRegion {
        &self.raw as *const onig_sys::OnigRegion as *mut onig_sys::OnigRegion
    }
}

impl Default for Region {
    fn default() -> Self {
        Region::new()
    }
}

impl Drop for Region {
    fn drop(&mut self) {
        unsafe {
            onig_sys::onig_region_free(&mut self.raw, 0);
        }
    }
}

impl Clone for Region {
    fn clone(&self) -> Self {
        unsafe { Self::clone_from_raw(self.raw_mut()) }
    }
}

impl<'a> IntoIterator for &'a Region {
    type Item = (usize, usize);
    type IntoIter = RegionIter<'a>;
    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

/// Region Iterator
///
/// This struct is responsible for holding iteration state over a
/// given region.
pub struct RegionIter<'a> {
    region: &'a Region,
    pos: usize,
}

impl<'a> Iterator for RegionIter<'a> {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let next = self.region.pos(self.pos);
        self.pos += 1;
        next
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.region.len();
        (len, Some(len))
    }

    fn count(self) -> usize {
        self.region.len()
    }
}

impl<'a> FusedIterator for RegionIter<'a> {}

impl<'a> ExactSizeIterator for RegionIter<'a> {}

#[cfg(test)]
mod tests {
    use super::super::{Regex, SearchOptions};
    use super::*;

    #[test]
    fn test_region_create() {
        Region::new();
    }

    #[test]
    fn test_region_clear() {
        let mut region = Region::new();
        region.clear();
    }

    #[test]
    fn test_region_copy() {
        let region = Region::new();
        let new_region = region.clone();
        assert_eq!(new_region.len(), region.len());
    }

    #[test]
    fn test_region_resize() {
        {
            let mut region = Region::new();
            assert!(region.capacity() == 0);
            region.reserve(100);
            {
                // can still get the capacity without a mutable borrow
                let region_borrowed = &region;
                assert!(region_borrowed.capacity() == 100);
            }
        }

        {
            let region = Region::with_capacity(10);
            assert!(region.capacity() == 10);
        }
    }

    #[test]
    fn test_region_empty_iterate() {
        let region = Region::new();
        for _ in &region {
            panic!("region should not contain any elements");
        }
    }

    #[test]
    fn test_region_iter_returns_iterator() {
        let region = Region::new();
        let all = region.iter().collect::<Vec<_>>();
        assert_eq!(all, Vec::new());
    }

    #[test]
    fn test_region_iterate_with_captures() {
        let mut region = Region::new();
        let reg = Regex::new("(a+)(b+)(c+)").unwrap();
        let res = reg.search_with_options(
            "aaaabbbbc",
            0,
            9,
            SearchOptions::SEARCH_OPTION_NONE,
            Some(&mut region),
        );
        assert!(res.is_some());
        let all = region.iter().collect::<Vec<_>>();
        assert_eq!(all, vec![(0, 9), (0, 4), (4, 8), (8, 9)]);
    }

    #[test]
    fn test_region_all_iteration_options() {
        let mut region = Region::new();
        let reg = Regex::new("a(b)").unwrap();
        let res = reg.search_with_options(
            "habitat",
            0,
            7,
            SearchOptions::SEARCH_OPTION_NONE,
            Some(&mut region),
        );
        assert!(res.is_some());

        // collect into a vector by iterating with a for loop
        let mut a = Vec::<(usize, usize)>::new();
        for pos in &region {
            a.push(pos)
        }

        // collect into a vector by using `iter` and collec
        let b = region.iter().collect::<Vec<_>>();

        let expected = vec![(1, 3), (2, 3)];
        assert_eq!(expected, a);
        assert_eq!(expected, b);

        assert_eq!(2, region.iter().count());
    }
}
