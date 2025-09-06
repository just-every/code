use std::os::raw::{c_int, c_void};
use std::slice;
use std::str::from_utf8_unchecked;

use onig_sys::{OnigRegex, OnigUChar};

use super::Regex;

impl Regex {
    /// Returns the number of named groups into regex.
    pub fn capture_names_len(&self) -> usize {
        unsafe { onig_sys::onig_number_of_names(self.raw) as usize }
    }

    /// Calls `callback` for each named group in the regex. Each callback gets the group name
    /// and group indices.
    pub fn foreach_name<F>(&self, mut callback: F) -> i32
    where
        F: FnMut(&str, &[u32]) -> bool,
    {
        unsafe extern "C" fn foreach_cb<F>(
            name: *const OnigUChar,
            name_end: *const OnigUChar,
            ngroup_num: c_int,
            group_nums: *mut c_int,
            _regex: OnigRegex,
            arg: *mut c_void,
        ) -> c_int
        where
            F: FnMut(&str, &[u32]) -> bool,
        {
            let name = from_utf8_unchecked(slice::from_raw_parts(
                name,
                name_end as usize - name as usize,
            ));

            let groups = slice::from_raw_parts(group_nums as *const u32, ngroup_num as usize);

            let callback = &mut *(arg as *mut F);

            if callback(name, groups) {
                0
            } else {
                -1
            }
        }

        unsafe {
            onig_sys::onig_foreach_name(
                self.raw,
                Some(foreach_cb::<F>),
                &mut callback as *mut F as *mut c_void,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;

    #[test]
    fn test_regex_names_len() {
        let regex = Regex::new("(he)(l+)(o)").unwrap();
        assert_eq!(regex.capture_names_len(), 0);
        let regex = Regex::new("(?<foo>he)(?<bar>l+)(?<bar>o)").unwrap();
        assert_eq!(regex.capture_names_len(), 2);
        assert_eq!(regex.capture_histories_len(), 0);
    }

    #[test]
    fn test_regex_names() {
        let regex = Regex::new("(he)(l+)(o)").unwrap();
        let mut names = Vec::new();
        regex.foreach_name(|n, i| {
            names.push((n.to_string(), i.iter().cloned().collect::<Vec<_>>()));
            true
        });
        assert_eq!(names, vec![]);
        let regex = Regex::new("(?<foo>he)(?<bar>l+)(?<bar>o)").unwrap();
        let mut names = Vec::new();
        regex.foreach_name(|n, i| {
            names.push((n.to_string(), i.iter().cloned().collect::<Vec<_>>()));
            true
        });
        assert_eq!(
            names,
            vec![("foo".into(), vec![1u32]), ("bar".into(), vec![2u32, 3])]
        );
    }
}
