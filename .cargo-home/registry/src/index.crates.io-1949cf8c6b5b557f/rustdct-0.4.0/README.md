# RustDCT
[![](https://img.shields.io/crates/v/rustdct.svg)](https://crates.io/crates/rustdct)
[![](https://img.shields.io/crates/l/rustdct.svg)](https://crates.io/crates/rustdct)
[![](https://docs.rs/rustdct/badge.svg)](https://docs.rs/rustdct/)
![minimum rustc 1.26](https://img.shields.io/badge/rustc-1.26+-red.svg)

RustDCT is a pure-Rust signal processing library that computes the most common Discrete Cosine Transforms: 

 * Discrete Cosine Transform (DCT) Types 1, 2, 3, 4
 * Discrete Sine Transform (DST) Types 1, 2, 3, 4
 * Modified Discrete Cosine Transform (MDCT)

## Example
```rust
// Compute a DCT Type 2 of size 1234
use rustdct::DCTplanner;

let mut input:  Vec<f32> = vec![0f32; 1234];
let mut output: Vec<f32> = vec![0f32; 1234];

let mut planner = DCTplanner::new();
let mut dct = planner.plan_dct2(1234);

dct.process(&mut input, &mut output);

```

## Compatibility
The `rustdct` crate requires rustc 1.26 or greater.

## Releases
Release notes are available in [RELEASES.md](RELEASES.md).
