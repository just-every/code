use rstest::*;

#[rstest]
#[case::description(42)]
fn with_case(#[context] ctx: Context, #[case] _c: u32) {
    assert_eq!("with_case", ctx.name);
    assert_eq!(Some("description"), ctx.description);
    assert_eq!(Some(0), ctx.case);
}

#[rstest]
fn without_case(#[context] ctx: Context) {
    assert_eq!("without_case", ctx.name);
    assert_eq!(None, ctx.description);
    assert_eq!(None, ctx.case);
}

mod first {
    mod inner {
        use rstest::*;

        #[rstest]
        fn test(#[context] ctx: Context) {
            assert!(ctx.module.ends_with("first::inner"));
        }
    }
}

#[rstest]
fn measure_time(#[context] ctx: Context) {
    std::thread::sleep(std::time::Duration::from_millis(100));
    assert!(ctx.start.elapsed() >= std::time::Duration::from_millis(100));
}
