# Release 0.4.0
 - Renamed `Type2and3` to `TransformType2And3`
 - Renamed `Type4` to `TransformType4`
 - Upgraded rustfft version from ^2.1 to ^3
 - Added a size-3 butterfly for TransformType2And3
# Release 0.3.0
 - Merged each DCT2 and DCT3 algorithm into a single struct that implements both both DCT2 and DCT3 traits, and created a "Type2And3" trait to encompass both. They both require the same precomputed data, so we can save memory and setup time by computing both fro mthe same trait.
 - Also implemented DST2 and DST3 on the Type2And3 trait -- so a single call to "plan_dct2" or "plan_dct3" etc will let you compute a DCT2, DST3, DCT3, DST3 all from the same instance.
 - DCT Type 4 instances can also compute DST Type 4. They implement the "Type4" trait, which includes both DCT4, and DST4.
 - Merged MDCT and IMDCT into the same trait
 - All of the above are breaking changes that will be very relevant to you if you're referring to specific algorithms or writing your own, but if you're just calling "plan_dct4" or etc, not much should be different.
 - Added a DST1 trait and added naive and FFT implementations
 - Added DST2, DST3, DST4 traits, and O(nlogn) implementations for each
# Release 0.2.1
 - Removed the `pub` keyword from some methods on the `DCTplanner` that should not have been public
# Release 0.2.0
 - All of the `DCT#ViaFFT` algorithms now allocate a Vec for internal scratch space, rather than using a member variable, allowing them to be immutable.
 - [Breaking Change] The `process` method of all DCT traits take `&self` instead of `&mut self`
 - [Breaking Change] Because all DCT instances are now immutable, all DCT instances that depend on other DCT instances take `Arc` pointers instead of `Box` pointers
 - [Breaking Change] Because all DCT instances are now immutable, the DCT planner now creates and stores `Arc` pointers to DCT instances instead of `Box`, and has a cache of algorithm instances for each DCT type
 - Implemented a "split radix" algorithm for DCT2 and DCT3 that processes power-of-two DCTs much faster than converting them to FFTs
 - Used the split radix alogirthm to derive several hardcoded "butterfly" algorithms for small DCT2 and DCT3 sizes (specifically, sizes 2,4,8,16)
 - [Breaking Change] Deleted the obsolete `DCT4ViaFFT` algorithm. For odd sizes, use `DCT4ViaFFTOdd` instead. For even sizes, use `DCT4ViaDCT3`. Both are considerably faster and use considerably less memory in all scenarios.
 - Lots of style improvements
# Prior releases
No prior release notes were kept.
