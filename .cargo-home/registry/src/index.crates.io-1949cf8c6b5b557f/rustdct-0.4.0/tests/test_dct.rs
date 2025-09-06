extern crate rustdct;
extern crate rand;

#[macro_use]
mod common;

use rustdct::{DCT1, DCT2, DCT3, DCT4, DST1, DST2, DST3, DST4};
use rustdct::algorithm::{DCT1Naive, DST1Naive, Type2And3Naive, Type4Naive};
use rustdct::mdct::window_fn;
use rustdct::DCTplanner;

use common::macros::test_mdct;
use common::known_data::*;
use common::slow_fns::*;
use common::{random_signal, compare_float_vectors};

#[test]
fn test_dct1_accuracy() {
    dct_test_with_known_data!(DCT1Naive, process_dct1, slow_dct1, known_values_dct1);
    dct_test_with_planner!(DCT1Naive, process_dct1, plan_dct1, 2);
}

#[test]
fn test_dct2_accuracy() {
    dct_test_with_known_data!(Type2And3Naive, process_dct2, slow_dct2, known_values_dct2);
    dct_test_with_planner!(Type2And3Naive, process_dct2, plan_dct2, 1);
}

#[test]
fn test_dct3_accuracy() {
    dct_test_with_known_data!(Type2And3Naive, process_dct3, slow_dct3, known_values_dct3);
    dct_test_with_planner!(Type2And3Naive, process_dct3, plan_dct3, 1);
}

#[test]
fn test_dct4_accuracy() {
    dct_test_with_known_data!(Type4Naive, process_dct4, slow_dct4, known_values_dct4);
    dct_test_with_planner!(Type4Naive, process_dct4, plan_dct4, 1);
}

#[test]
fn test_dst1_accuracy() {
    dct_test_with_known_data!(DST1Naive, process_dst1, slow_dst1, known_values_dst1);
}

#[test]
fn test_dst2_accuracy() {
    dct_test_with_known_data!(Type2And3Naive, process_dst2, slow_dst2, known_values_dst2);
}

#[test]
fn test_dst3_accuracy() {
    dct_test_with_known_data!(Type2And3Naive, process_dst3, slow_dst3, known_values_dst3);
}

#[test]
fn test_dst4_accuracy() {
    dct_test_with_known_data!(Type4Naive, process_dst4, slow_dst4, known_values_dst4);
}


#[test]
fn test_mdct_accuracy() {
    for curent_window_fn in &[window_fn::one, window_fn::mp3, window_fn::vorbis] {
        for len in 1..10 {
            test_mdct::planned_matches_naive(len * 2, curent_window_fn);
        }
        for &len in &[50, 52] {
            test_mdct::planned_matches_naive(len * 2, curent_window_fn);
        }
    }
}

#[test]
fn test_mdct_tdac() {
    struct TdacTestStruct<'a> {
        name: &'static str,
        window: &'a dyn Fn(usize) -> Vec<f32>,
        scale_fn: &'a dyn Fn(usize) -> f32,
    }
    impl<'a> TdacTestStruct<'a> {
        fn new(name: &'static str, window: &'a dyn Fn(usize) -> Vec<f32>, scale_fn: &'a dyn Fn(usize) -> f32) -> Self {
            Self { name, window, scale_fn }
        }
    }

    let non_window_scale = |len: usize| 1.0/(len as f32);
    let window_scale = |len : usize| 2.0/(len as f32);
    let invertible_scale = |_| 1.0;

    let tests = [
        TdacTestStruct::new("one",                  &window_fn::one,                &non_window_scale),
        TdacTestStruct::new("mp3",                  &window_fn::mp3,                &window_scale),
        TdacTestStruct::new("vorbis",               &window_fn::vorbis,             &window_scale),
        TdacTestStruct::new("invertible",           &window_fn::invertible,         &invertible_scale),
        TdacTestStruct::new("mp3_invertible",       &window_fn::mp3_invertible,     &invertible_scale),
        TdacTestStruct::new("vorbis_invertible",    &window_fn::vorbis_invertible,  &invertible_scale),
    ];

    for test_data in &tests {
        for i in 1..10 {
            let len = i * 2;
            println!("name: {}, len: {}", test_data.name, len);
            test_mdct::test_tdac(len, (test_data.scale_fn)(len), test_data.window);
        }
        for &i in &[50, 52] {
            let len = i * 2;
            println!("name: {}, len: {}", test_data.name, len);
            test_mdct::test_tdac(len, (test_data.scale_fn)(len), test_data.window);
        }
    }
}
