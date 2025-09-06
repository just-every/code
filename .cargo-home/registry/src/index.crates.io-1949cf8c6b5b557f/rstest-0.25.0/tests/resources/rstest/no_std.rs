#![no_std]

use rstest::{fixture, rstest};

#[fixture]
#[once]
fn once_fixture() -> u64 {
    42
}

#[fixture]
#[once]
fn empty_once_fixture() {}

#[fixture]
async fn async_fixture() -> u64 {
    42
}

pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[rstest]
#[case("2", "2", "4")]
fn it_works(#[case] left: u64, #[case] right: u64, #[case] expected: u64) {
    assert_eq!(add(left, right), expected);
}

#[rstest]
async fn async_works(#[future] async_fixture: u64) {
    assert_eq!(42, async_fixture.await);
}
