#![feature(test)]
extern crate test;
extern crate rustdct;

use std::sync::Arc;

use rustdct::rustfft::FFTplanner;
use rustdct::DCTplanner;
use rustdct::{DCT1, DCT2, DCT3, DCT4, TransformType2And3};
use rustdct::algorithm::*;
use rustdct::algorithm::type2and3_butterflies::*;
use rustdct::mdct::{MDCT, MDCTViaDCT4, window_fn};

use test::Bencher;

/// Times just the DCT1 execution (not allocation and pre-calculation)
/// for a given length
fn bench_dct1_fft(b: &mut Bencher, len: usize) {

    let mut planner = FFTplanner::new(false);
    let dct = DCT1ConvertToFFT::new(planner.plan_fft((len - 1) * 2));

    let mut signal = vec![0_f32; len];
    let mut spectrum = signal.clone();
    b.iter(|| { dct.process_dct1(&mut signal, &mut spectrum); });
}

#[bench]
fn dct1_fft_0024(b: &mut Bencher) {
    bench_dct1_fft(b, 24);
}
#[bench]
fn dct1_fft_0025(b: &mut Bencher) {
    bench_dct1_fft(b, 25);
}
#[bench]
fn dct1_fft_0026(b: &mut Bencher) {
    bench_dct1_fft(b, 26);
}



/// Times just the DCT2 execution (not allocation and pre-calculation)
/// for a given length
fn bench_dct2_fft(b: &mut Bencher, len: usize) {

    let mut planner = FFTplanner::new(false);
    let dct = Type2And3ConvertToFFT::new(planner.plan_fft(len));

    let mut signal = vec![0_f32; len];
    let mut spectrum = signal.clone();
    b.iter(|| { dct.process_dct2(&mut signal, &mut spectrum); });
}


#[bench]
fn dct2_fft_017(b: &mut Bencher) {
    bench_dct2_fft(b, 17);
}
#[bench]
fn dct2_fft_018(b: &mut Bencher) {
    bench_dct2_fft(b, 18);
}
#[bench]
fn dct2_fft_019(b: &mut Bencher) {
    bench_dct2_fft(b, 19);
}
#[bench]
fn dct2_fft_020(b: &mut Bencher) {
    bench_dct2_fft(b, 20);
}
#[bench]
fn dct2_fft_021(b: &mut Bencher) {
    bench_dct2_fft(b, 21);
}
#[bench]
fn dct2_fft_022(b: &mut Bencher) {
    bench_dct2_fft(b, 22);
}


/// Times just the DCT2 execution (not allocation and pre-calculation)
/// for a given length
fn bench_dct2_split(b: &mut Bencher, len: usize) {

    let power = len.trailing_zeros() as usize;
    let mut instances = vec![
        Arc::new(Type2And3Naive::new(1)) as Arc<TransformType2And3<f32>>,
        Arc::new(Type2And3Butterfly2::new()) as Arc<TransformType2And3<f32>>,
        Arc::new(Type2And3Butterfly4::new()) as Arc<TransformType2And3<f32>>,
        Arc::new(Type2And3Butterfly8::new()) as Arc<TransformType2And3<f32>>,
        Arc::new(Type2And3Butterfly16::new()) as Arc<TransformType2And3<f32>>,
    ];
    for i in instances.len()..(power + 1) {
        let dct = Arc::new(Type2And3SplitRadix::new(instances[i - 1].clone(), instances[i - 2].clone()));
        instances.push(dct);
    }

    let dct = instances[power].clone();
    assert_eq!(dct.len(), len);

    let mut signal = vec![0_f32; len];
    let mut spectrum = signal.clone();
    b.iter(|| { dct.process_dct2(&mut signal, &mut spectrum); });
}
#[bench]
fn dct2_power2_split_0002(b: &mut Bencher) {
    bench_dct2_split(b, 2);
}
#[bench]
fn dct2_power2_split_0004(b: &mut Bencher) {
    bench_dct2_split(b, 4);
}
#[bench]
fn dct2_power2_split_0008(b: &mut Bencher) {
    bench_dct2_split(b, 4);
}
#[bench]
fn dct2_power2_split_0016(b: &mut Bencher) {
    bench_dct2_split(b, 16);
}
#[bench]
fn dct2_power2_split_0064(b: &mut Bencher) {
    bench_dct2_split(b, 64);
}
#[bench]
fn dct2_power2_split_0256(b: &mut Bencher) {
    bench_dct2_split(b, 256);
}
#[bench]
fn dct2_power2_split_065536(b: &mut Bencher) {
    bench_dct2_split(b, 65536);
}





/// Times just the DCT3 execution (not allocation and pre-calculation)
/// for a given length
fn bench_dct3_fft(b: &mut Bencher, len: usize) {

    let mut planner = FFTplanner::new(false);
    let dct = Type2And3ConvertToFFT::new(planner.plan_fft(len));

    let mut signal = vec![0_f32; len];
    let mut spectrum = signal.clone();
    b.iter(|| { dct.process_dct3(&mut signal, &mut spectrum); });
}

#[bench]
fn dct3_fft_003(b: &mut Bencher) {
    bench_dct3_fft(b, 3);
}
#[bench]
fn dct3_fft_004(b: &mut Bencher) {
    bench_dct3_fft(b, 4);
}
#[bench]
fn dct3_fft_005(b: &mut Bencher) {
    bench_dct3_fft(b, 5);
}
#[bench]
fn dct3_fft_006(b: &mut Bencher) {
    bench_dct3_fft(b, 6);
}

#[bench]
fn dct3_power2_fft_00004(b: &mut Bencher) {
    bench_dct3_fft(b, 4);
}
#[bench]
fn dct3_power2_fft_00008(b: &mut Bencher) {
    bench_dct3_fft(b, 8);
}
#[bench]
fn dct3_power2_fft_00016(b: &mut Bencher) {
    bench_dct3_fft(b, 16);
}
#[bench]
fn dct3_power2_fft_00032(b: &mut Bencher) {
    bench_dct3_fft(b, 32);
}
#[bench]
fn dct3_power2_fft_00064(b: &mut Bencher) {
    bench_dct3_fft(b, 64);
}
#[bench]
fn dct3_power2_fft_00256(b: &mut Bencher) {
    bench_dct3_fft(b, 256);
}
#[bench]
fn dct3_power2_fft_065536(b: &mut Bencher) {
    bench_dct3_fft(b, 65536);
}
#[bench]
fn dct3_power2_fft_16777216(b: &mut Bencher) {
    bench_dct3_fft(b, 16777216);
}

/// Times just the DCT2 execution (not allocation and pre-calculation)
/// for a given length
fn bench_dct3_split(b: &mut Bencher, len: usize) {

    let power = len.trailing_zeros() as usize;
    let mut instances = vec![
        Arc::new(Type2And3Naive::new(1)) as Arc<TransformType2And3<f32>>,
        Arc::new(Type2And3Butterfly2::new()) as Arc<TransformType2And3<f32>>,
        Arc::new(Type2And3Butterfly4::new()) as Arc<TransformType2And3<f32>>,
        Arc::new(Type2And3Butterfly8::new()) as Arc<TransformType2And3<f32>>,
        Arc::new(Type2And3Butterfly16::new()) as Arc<TransformType2And3<f32>>,
    ];
    for i in instances.len()..(power + 1) {
        let dct = Arc::new(Type2And3SplitRadix::new(instances[i - 1].clone(), instances[i - 2].clone()));
        instances.push(dct);
    }

    let dct = instances[power].clone();
    assert_eq!(dct.len(), len);

    let mut signal = vec![0_f32; len];
    let mut spectrum = signal.clone();
    b.iter(|| { dct.process_dct3(&mut signal, &mut spectrum); });
}
#[bench]
fn dct3_power2_split_0002(b: &mut Bencher) {
    bench_dct3_split(b, 4);
}
#[bench]
fn dct3_power2_split_0004(b: &mut Bencher) {
    bench_dct3_split(b, 4);
}
#[bench]
fn dct3_power2_split_0008(b: &mut Bencher) {
    bench_dct3_split(b, 4);
}
#[bench]
fn dct3_power2_split_0016(b: &mut Bencher) {
    bench_dct3_split(b, 16);
}
#[bench]
fn dct3_power2_split_0064(b: &mut Bencher) {
    bench_dct3_split(b, 64);
}
#[bench]
fn dct3_power2_split_0256(b: &mut Bencher) {
    bench_dct3_split(b, 256);
}
#[bench]
fn dct3_power2_split_065536(b: &mut Bencher) {
    bench_dct3_split(b, 65536);
}



/// Times just the DCT4 execution (not allocation and pre-calculation)
/// for a given length
fn bench_dct4_via_dct3(b: &mut Bencher, len: usize) {

    let mut planner = DCTplanner::new();
    let inner_dct3 = planner.plan_dct3(len / 2);
    let dct = Type4ConvertToType3Even::new(inner_dct3);

    let mut signal = vec![0_f32; len];
    let mut spectrum = signal.clone();
    b.iter(|| { dct.process_dct4(&mut signal, &mut spectrum); });
}

#[bench]
fn dct4_even_via_dct3_02(b: &mut Bencher) {
    bench_dct4_via_dct3(b, 2);
}
#[bench]
fn dct4_even_via_dct3_04(b: &mut Bencher) {
    bench_dct4_via_dct3(b, 4);
}
#[bench]
fn dct4_even_via_dct3_06(b: &mut Bencher) {
    bench_dct4_via_dct3(b, 6);
}
#[bench]
fn dct4_even_via_dct3_08(b: &mut Bencher) {
    bench_dct4_via_dct3(b, 8);
}
#[bench]
fn dct4_even_via_dct3_10(b: &mut Bencher) {
    bench_dct4_via_dct3(b, 10);
}

#[bench]
fn dct4_even_via_dct3_1000000(b: &mut Bencher) {
    bench_dct4_via_dct3(b, 1000000);
}


/// Times just the DCT4 execution (not allocation and pre-calculation)
/// for a given length
fn bench_dct4_via_fft_odd(b: &mut Bencher, len: usize) {

    let mut planner = FFTplanner::new(false);
    let inner_fft = planner.plan_fft(len);
    let dct = Type4ConvertToFFTOdd::new(inner_fft);

    let mut signal = vec![0_f32; len];
    let mut spectrum = signal.clone();
    b.iter(|| { dct.process_dct4(&mut signal, &mut spectrum); });
}

#[bench]
fn dct4_odd_via_fft_01(b: &mut Bencher) {
    bench_dct4_via_fft_odd(b, 1);
}
#[bench]
fn dct4_odd_via_fft_03(b: &mut Bencher) {
    bench_dct4_via_fft_odd(b, 3);
}
#[bench]
fn dct4_odd_via_fft_05(b: &mut Bencher) {
    bench_dct4_via_fft_odd(b, 5);
}
#[bench]
fn dct4_odd_via_fft_07(b: &mut Bencher) {
    bench_dct4_via_fft_odd(b, 7);
}
#[bench]
fn dct4_odd_via_fft_09(b: &mut Bencher) {
    bench_dct4_via_fft_odd(b, 9);
}
#[bench]
fn dct4_odd_via_fft_999999(b: &mut Bencher) {
    bench_dct4_via_fft_odd(b, 999999);
}




/// Times just the MDCT execution (not allocation and pre-calculation)
/// for a given length
fn bench_mdct_fft(b: &mut Bencher, len: usize) {

    let mut planner = DCTplanner::new();
    let dct = MDCTViaDCT4::new(planner.plan_dct4(len), window_fn::mp3);

    let signal = vec![0_f32; len * 2];
    let mut spectrum = vec![0_f32; len];
    b.iter(|| { dct.process_mdct(&signal, &mut spectrum); });
}
#[bench]
fn mdct_fft_02(b: &mut Bencher) {
    bench_mdct_fft(b, 2);
}
#[bench]
fn mdct_fft_04(b: &mut Bencher) {
    bench_mdct_fft(b, 4);
}
#[bench]
fn mdct_fft_06(b: &mut Bencher) {
    bench_mdct_fft(b, 6);
}
#[bench]
fn mdct_fft_08(b: &mut Bencher) {
    bench_mdct_fft(b, 8);
}
#[bench]
fn mdct_fft_10(b: &mut Bencher) {
    bench_mdct_fft(b, 10);
}
#[bench]
fn mdct_fft_12(b: &mut Bencher) {
    bench_mdct_fft(b, 12);
}




/// Times just the IMDCT execution (not allocation and pre-calculation)
/// for a given length
fn bench_imdct_fft(b: &mut Bencher, len: usize) {

    let mut planner = DCTplanner::new();
    let dct = MDCTViaDCT4::new(planner.plan_dct4(len), window_fn::mp3);

    let signal = vec![0_f32; len];
    let mut spectrum = vec![0_f32; len * 2];
    b.iter(|| { dct.process_imdct(&signal, &mut spectrum); });
}
#[bench]
fn imdct_fft_02(b: &mut Bencher) {
    bench_imdct_fft(b, 2);
}
#[bench]
fn imdct_fft_04(b: &mut Bencher) {
    bench_imdct_fft(b, 4);
}
#[bench]
fn imdct_fft_06(b: &mut Bencher) {
    bench_imdct_fft(b, 6);
}
#[bench]
fn imdct_fft_08(b: &mut Bencher) {
    bench_imdct_fft(b, 8);
}
#[bench]
fn imdct_fft_10(b: &mut Bencher) {
    bench_imdct_fft(b, 10);
}
#[bench]
fn imdct_fft_12(b: &mut Bencher) {
    bench_imdct_fft(b, 12);
}
