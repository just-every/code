#![feature(test)]
extern crate test;

extern crate transpose;

fn bench_transpose<T: Copy + Default>(b: &mut test::Bencher, width: usize, height: usize) {
    
    let mut buffer = vec![T::default(); width * height];
    let mut scratch = vec![T::default(); width * height];

    b.iter(|| { transpose::transpose(&mut buffer, &mut scratch, width, height); });
}

#[bench] fn bench1_u8_0004x0004(b: &mut test::Bencher) { bench_transpose::<u8>(b, 4, 4); }
#[bench] fn bench1_u8_0008x0008(b: &mut test::Bencher) { bench_transpose::<u8>(b, 8, 8); }
#[bench] fn bench1_u8_0016x0016(b: &mut test::Bencher) { bench_transpose::<u8>(b, 16, 16); }
#[bench] fn bench1_u8_0064x0064(b: &mut test::Bencher) { bench_transpose::<u8>(b, 64, 64); }
#[bench] fn bench1_u8_0256x0256(b: &mut test::Bencher) { bench_transpose::<u8>(b, 256, 256); }
#[bench] fn bench1_u8_1024x1024(b: &mut test::Bencher) { bench_transpose::<u8>(b, 1024, 1024); }
#[bench] fn bench1_u8_4096x4096(b: &mut test::Bencher) { bench_transpose::<u8>(b, 4096, 4096); }

#[bench] fn bench2_u32_0004x0004(b: &mut test::Bencher) { bench_transpose::<u32>(b, 4, 4); }
#[bench] fn bench2_u32_0008x0008(b: &mut test::Bencher) { bench_transpose::<u32>(b, 8, 8); }
#[bench] fn bench2_u32_0016x0016(b: &mut test::Bencher) { bench_transpose::<u32>(b, 16, 16); }
#[bench] fn bench2_u32_0064x0064(b: &mut test::Bencher) { bench_transpose::<u32>(b, 64, 64); }
#[bench] fn bench2_u32_0256x0256(b: &mut test::Bencher) { bench_transpose::<u32>(b, 256, 256); }
#[bench] fn bench2_u32_1024x1024(b: &mut test::Bencher) { bench_transpose::<u32>(b, 1024, 1024); }
#[bench] fn bench2_u32_4096x4096(b: &mut test::Bencher) { bench_transpose::<u32>(b, 4096, 4096); }

#[bench] fn bench3_u64_0004x0004(b: &mut test::Bencher) { bench_transpose::<u64>(b, 4, 4); }
#[bench] fn bench3_u64_0008x0008(b: &mut test::Bencher) { bench_transpose::<u64>(b, 8, 8); }
#[bench] fn bench3_u64_0016x0016(b: &mut test::Bencher) { bench_transpose::<u64>(b, 16, 16); }
#[bench] fn bench3_u64_0064x0064(b: &mut test::Bencher) { bench_transpose::<u64>(b, 64, 64); }
#[bench] fn bench3_u64_0256x0256(b: &mut test::Bencher) { bench_transpose::<u64>(b, 256, 256); }
#[bench] fn bench3_u64_1024x1024(b: &mut test::Bencher) { bench_transpose::<u64>(b, 1024, 1024); }
#[bench] fn bench3_u64_4096x4096(b: &mut test::Bencher) { bench_transpose::<u64>(b, 4096, 4096); }

#[bench] fn bench4_u128_0004x0004(b: &mut test::Bencher) { bench_transpose::<u128>(b, 4, 4); }
#[bench] fn bench4_u128_0008x0008(b: &mut test::Bencher) { bench_transpose::<u128>(b, 8, 8); }
#[bench] fn bench4_u128_0016x0016(b: &mut test::Bencher) { bench_transpose::<u128>(b, 16, 16); }
#[bench] fn bench4_u128_0064x0064(b: &mut test::Bencher) { bench_transpose::<u128>(b, 64, 64); }
#[bench] fn bench4_u128_0256x0256(b: &mut test::Bencher) { bench_transpose::<u128>(b, 256, 256); }
#[bench] fn bench4_u128_1024x1024(b: &mut test::Bencher) { bench_transpose::<u128>(b, 1024, 1024); }
#[bench] fn bench4_u128_4096x4096(b: &mut test::Bencher) { bench_transpose::<u128>(b, 4096, 4096); }