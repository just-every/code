//! Read from the global pasteboard, and write a new string into it.
//!
//! Works on macOS 10.7+
#![deny(unsafe_op_in_unsafe_fn)]

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::ClassType;
use objc2_app_kit::{NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::{NSArray, NSString};

/// Simplest implementation
pub fn get_text_1(pasteboard: &NSPasteboard) -> Option<Retained<NSString>> {
    unsafe { pasteboard.stringForType(NSPasteboardTypeString) }
}

/// More complex implementation using `readObjectsForClasses:options:`,
/// intended to show how some patterns might require more work.
pub fn get_text_2(pasteboard: &NSPasteboard) -> Option<Retained<NSString>> {
    let class_array = NSArray::from_slice(&[NSString::class()]);
    let objects = unsafe { pasteboard.readObjectsForClasses_options(&class_array, None) };

    let obj = objects?.firstObject()?;
    // We now have to convert the object into an NSString, which we know it to
    // be since that's what we told `readObjectsForClasses:options:` we wanted.
    Some(obj.downcast::<NSString>().unwrap())
}

pub fn set_text(pasteboard: &NSPasteboard, text: &NSString) {
    let _ = unsafe { pasteboard.clearContents() };
    let obj = ProtocolObject::from_ref(text);
    let objects = NSArray::from_slice(&[obj]);
    let res = unsafe { pasteboard.writeObjects(&objects) };
    if !res {
        panic!("Failed writing to pasteboard");
    }
}

fn main() {
    let pasteboard = unsafe { NSPasteboard::generalPasteboard() };
    let impl_1 = get_text_1(&pasteboard);
    let impl_2 = get_text_2(&pasteboard);
    println!("Pasteboard text from implementation 1 was: {impl_1:?}");
    println!("Pasteboard text from implementation 2 was: {impl_2:?}");
    assert_eq!(impl_1, impl_2);

    let s = NSString::from_str("Hello, world!");
    set_text(&pasteboard, &s);
    println!("Now the pasteboard text should be: {s:?}");
    assert_eq!(Some(s), get_text_1(&pasteboard));
}
