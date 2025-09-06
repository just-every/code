## [3.0.1]
### Fixed
- Fixed warnings regarding "dyn trait", and warnings regarding inclusive ranges
- Several documentation improvements

## [3.0.0]
### Changed
- Reduced the setup time and memory usage of GoodThomasAlgorithm
- Reduced the setup time and memory usage of RadersAlgorithm

### Breaking Changes
- Documented the minimum rustsc version. Before, none was specified. now, it's 1.26. Further increases to minimum version will be a breaking change.
- Increased the version of the num-complex dependency to 0.2. This is a breaking change because we have a public dependency on num-complex.
See the [num-complex changelog](https://github.com/rust-num/num-complex/blob/master/RELEASES.md) for a list of breaking changes in num-complex 0.2

## [2.1.0]
### Added
- Added a specialized implementation of Good Thomas Algorithm for when both inner FFTs are butterflies. (#33)

### Changed
- Documentation typo fixes (#27, #35)
- Increased minimum version of num_traits and num_complex. Notably, Complex<T> is now guaranteed to be repr(C)
- Significantly improved the performance of the Radix4 algorithm (#26)
- Reduced memory usage of prime-sized FFTs (#34)
- Incorporated the Good-Thomas Double Butterfly algorithm into the planner, improving performance for most composite and prime FFTs

## [2.0.0]
### Added
- Added implementation of Good Thomas algorithm.
- Added implementation of Raders algorithm.
- Added implementation of Radix algorithm for power-of-two lengths.
- Added `FFTPlanner` to choose the fastest algorithm for a given size.

### Changed
- Changed API to take the "signal" as mutable and use it for scratch space.

## [1.0.1]
### Changed
- Relicensed to dual MIT/Apache-2.0.

## [1.0.0]
### Added
- Added initial implementation of Cooley-Tukey.
