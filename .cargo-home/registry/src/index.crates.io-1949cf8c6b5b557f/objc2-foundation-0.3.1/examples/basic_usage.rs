use objc2_foundation::{ns_string, NSArray, NSDictionary, NSObject};

fn main() {
    // Create and compare NSObjects
    let obj = NSObject::new();
    #[allow(clippy::eq_op)]
    {
        println!("{obj:?} == {obj:?}? {:?}", obj == obj);
    }

    let obj2 = NSObject::new();
    println!("{obj:?} == {obj2:?}? {:?}", obj == obj2);

    // Create an NSArray from a Vec
    let objs = vec![obj, obj2];
    let array = NSArray::from_retained_slice(&objs);
    for obj in array.iter() {
        println!("{obj:?}");
    }
    println!("{}", array.len());

    // Turn the NSArray back into a Vec
    let mut objs = array.to_vec();
    let obj = objs.pop().unwrap();

    // Create a static NSString
    let string = ns_string!("Hello, world!");
    // And use the `ToString` implementation to convert it into a string
    let _s = string.to_string();
    // Or use the `Display` implementation directly
    println!("{string}");

    // Create a dictionary mapping strings to objects
    let keys = &[string];
    let objects = &[&*obj];
    let dict = NSDictionary::from_slices(keys, objects);
    println!("{:?}", dict.objectForKey(string));
    println!("{}", dict.len());
}
