use std::collections::HashMap;
use std::sync::Arc;

use rustfft::FFTplanner;
use common;
use ::{DCT1, DST1, TransformType4, TransformType2And3};
use mdct::*;
use algorithm::*;
use algorithm::type2and3_butterflies::*;

const DCT2_BUTTERFLIES: [usize; 5] = [2, 3, 4, 8, 16];

/// The DCT planner is used to make new DCT algorithm instances.
///
/// RustDCT has several DCT algorithms available for each DCT type; For a given DCT type and problem size, the FFTplanner
/// decides which of the available DCT algorithms to use and then initializes them.
///
/// ~~~
/// // Perform a DCT Type 4 of size 1234
/// use std::sync::Arc;
/// use rustdct::DCTplanner;
///
/// let mut input:  Vec<f32> = vec![0f32; 1234];
/// let mut output: Vec<f32> = vec![0f32; 1234];
///
/// let mut planner = DCTplanner::new();
/// let dct4 = planner.plan_dct4(1234);
/// dct4.process_dct4(&mut input, &mut output);
/// 
/// // The DCT instance returned by the planner is stored behind an `Arc`, so it's cheap to clone
/// let dct4_clone = Arc::clone(&dct4);
/// ~~~
///
/// If you plan on creating multiple DCT instances, it is recommnded to reuse the same planner for all of them. This
/// is because the planner re-uses internal data across DCT instances wherever possible, saving memory and reducing
/// setup time. (DCT instances created with one planner will never re-use data and buffers with DCT instances created
/// by a different planner)
///
/// Each DCT instance owns `Arc`s to its shared internal data, rather than borrowing it from the planner, so it's
/// perfectly safe to drop the planner after creating DCT instances.
pub struct DCTplanner<T> {
    fft_planner: FFTplanner<T>,
    dct1_cache: HashMap<usize, Arc<DCT1<T>>>,
    dst1_cache: HashMap<usize, Arc<DST1<T>>>,
    dct23_cache: HashMap<usize, Arc<TransformType2And3<T>>>,
    dct4_cache: HashMap<usize, Arc<TransformType4<T>>>,
    mdct_cache: HashMap<usize, Arc<MDCT<T>>>,
}
impl<T: common::DCTnum> DCTplanner<T> {
    pub fn new() -> Self {
        Self {
            fft_planner: FFTplanner::new(false),
            dct1_cache: HashMap::new(),
            dst1_cache: HashMap::new(),
            dct23_cache: HashMap::new(),
            dct4_cache: HashMap::new(),
            mdct_cache: HashMap::new(),
        }
    }

    /// Returns a DCT Type 1 instance which processes signals of size `len`.
    /// If this is called multiple times, it will attempt to re-use internal data between instances
    pub fn plan_dct1(&mut self, len: usize) -> Arc<DCT1<T>> {
        if self.dct1_cache.contains_key(&len) {
            Arc::clone(self.dct1_cache.get(&len).unwrap())
        } else {
            let result = self.plan_new_dct1(len);
            self.dct1_cache.insert(len, Arc::clone(&result));
            result
        }
    }

    fn plan_new_dct1(&mut self, len: usize) -> Arc<DCT1<T>> {
        //benchmarking shows that below about 25, it's faster to just use the naive DCT1 algorithm
        if len < 25 {
            Arc::new(DCT1Naive::new(len))
        } else {
            let fft = self.fft_planner.plan_fft((len - 1) * 2);
            Arc::new(DCT1ConvertToFFT::new(fft))
        }
    }




    /// Returns a DCT Type 2 instance which processes signals of size `len`.
    /// If this is called multiple times, it will attempt to re-use internal data between instances
    pub fn plan_dct2(&mut self, len: usize) -> Arc<TransformType2And3<T>> {
        if self.dct23_cache.contains_key(&len) {
            Arc::clone(self.dct23_cache.get(&len).unwrap())
        } else {
            let result = self.plan_new_dct2(len);
            self.dct23_cache.insert(len, Arc::clone(&result));
            result
        }
    }

    fn plan_new_dct2(&mut self, len: usize) -> Arc<TransformType2And3<T>> {
        if DCT2_BUTTERFLIES.contains(&len) {
            self.plan_dct2_butterfly(len)
        } else if len.is_power_of_two() && len > 2 {
            let half_dct = self.plan_dct2(len / 2);
            let quarter_dct = self.plan_dct2(len / 4);
            Arc::new(Type2And3SplitRadix::new(half_dct, quarter_dct))
        } else if len < 16 {
            //benchmarking shows that below about 16, it's faster to just use the naive DCT2 algorithm
            Arc::new(Type2And3Naive::new(len))
        } else {
            let fft = self.fft_planner.plan_fft(len);
            Arc::new(Type2And3ConvertToFFT::new(fft))
        }
    }

    fn plan_dct2_butterfly(&mut self, len: usize) -> Arc<TransformType2And3<T>> {
        match len {
            2 => Arc::new(Type2And3Butterfly2::new()),
            3 => Arc::new(Type2And3Butterfly3::new()),
            4 => Arc::new(Type2And3Butterfly4::new()),
            8 => Arc::new(Type2And3Butterfly8::new()),
            16 => Arc::new(Type2And3Butterfly16::new()),
            _ => panic!("Invalid butterfly size for DCT2: {}", len)
        }
    }




    /// Returns DCT Type 3 instance which processes signals of size `len`.
    /// If this is called multiple times, it will attempt to re-use internal data between instances
    pub fn plan_dct3(&mut self, len: usize) -> Arc<TransformType2And3<T>> {
        self.plan_dct2(len)
    }
    
    /// Returns a DCT Type 4 instance which processes signals of size `len`.
    /// If this is called multiple times, it will attempt to re-use internal data between instances
    pub fn plan_dct4(&mut self, len: usize) -> Arc<TransformType4<T>> {
        if self.dct4_cache.contains_key(&len) {
            Arc::clone(self.dct4_cache.get(&len).unwrap())
        } else {
            let result = self.plan_new_dct4(len);
            self.dct4_cache.insert(len, Arc::clone(&result));
            result
        }
    }

    fn plan_new_dct4(&mut self, len: usize) -> Arc<TransformType4<T>> {
        //if we have an even size, we can use the DCT4 Via DCT3 algorithm
        if len % 2 == 0 {
            //benchmarking shows that below 6, it's faster to just use the naive DCT4 algorithm
            if len < 6 {
                Arc::new(Type4Naive::new(len))
            } else {
                let inner_dct = self.plan_dct3(len / 2);
                Arc::new(Type4ConvertToType3Even::new(inner_dct))
            }
        } else {
            //odd size, so we can use the "DCT4 via FFT odd" algorithm
            //benchmarking shows that below about 7, it's faster to just use the naive DCT4 algorithm
            if len < 7 {
                Arc::new(Type4Naive::new(len))
            } else {
                let fft = self.fft_planner.plan_fft(len);
                Arc::new(Type4ConvertToFFTOdd::new(fft))
            }
        }
    }

    /// Returns a DST Type 1 instance which processes signals of size `len`.
    /// If this is called multiple times, it will attempt to re-use internal data between instances
    pub fn plan_dst1(&mut self, len: usize) -> Arc<DST1<T>> {
        if self.dst1_cache.contains_key(&len) {
            Arc::clone(self.dst1_cache.get(&len).unwrap())
        } else {
            let result = self.plan_new_dst1(len);
            self.dst1_cache.insert(len, Arc::clone(&result));
            result
        }
    }

    fn plan_new_dst1(&mut self, len: usize) -> Arc<DST1<T>> {
        //benchmarking shows that below about 25, it's faster to just use the naive DCT1 algorithm
        if len < 25 {
            Arc::new(DST1Naive::new(len))
        } else {
            let fft = self.fft_planner.plan_fft((len + 1) * 2);
            Arc::new(DST1ConvertToFFT::new(fft))
        }
    }

    /// Returns DST Type 2 instance which processes signals of size `len`.
    /// If this is called multiple times, it will attempt to re-use internal data between instances
    pub fn plan_dst2(&mut self, len: usize) -> Arc<TransformType2And3<T>> {
        self.plan_dct2(len)
    }

    /// Returns DST Type 3 instance which processes signals of size `len`.
    /// If this is called multiple times, it will attempt to re-use internal data between instances
    pub fn plan_dst3(&mut self, len: usize) -> Arc<TransformType2And3<T>> {
        self.plan_dct2(len)
    }

    /// Returns DST Type 4 instance which processes signals of size `len`.
    /// If this is called multiple times, it will attempt to re-use internal data between instances
    pub fn plan_dst4(&mut self, len: usize) -> Arc<TransformType4<T>> {
        self.plan_dct4(len)
    }


    /// Returns a MDCT instance which processes inputs of size ` len * 2` and produces outputs of size `len`.
    ///
    /// `window_fn` is a function that takes a `size` and returns a `Vec` containing `size` window values.
    /// See the [`window_fn`](mdct/window_fn/index.html) module for provided window functions.
    ///
    /// If this is called multiple times, it will attempt to re-use internal data between instances
    pub fn plan_mdct<F>(&mut self, len: usize, window_fn: F) -> Arc<MDCT<T>>
    where F: (FnOnce(usize) -> Vec<T>) {
        if self.mdct_cache.contains_key(&len) {
            Arc::clone(self.mdct_cache.get(&len).unwrap())
        } else {
            let result = self.plan_new_mdct(len, window_fn);
            self.mdct_cache.insert(len, Arc::clone(&result));
            result
        }
    }

    fn plan_new_mdct<F>(&mut self, len: usize, window_fn: F) -> Arc<MDCT<T>>
    where F: (FnOnce(usize) -> Vec<T>) {
        //benchmarking shows that using the inner dct4 algorithm is always faster than computing the naive algorithm
        let inner_dct4 = self.plan_dct4(len);
        Arc::new(MDCTViaDCT4::new(inner_dct4, window_fn))
    }
}
