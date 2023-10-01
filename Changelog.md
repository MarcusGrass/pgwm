# Change Log
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).
## [Unreleased]
### Fixed

### Added

### Changed

## [v0.6.0] - 2023-10-01

### Fixed

### Added

### Changed
- Update tiny-std to `0.2.1` 
- Remove makefile just use `cargo`
- Remove old dockerfiles

## [v0.5.0]
### Fixed
- Update all deps

### Added

### Changed
- Removed config file, running config-as-code instead, 
reducing binary-size dramatically.  
- Updated dependencies, and removed dependency on some 
serialization libraries.  

## [v0.4.0] - 2023-06-11
### Fixed

### Added

### Changed
- Run all io through io-uring
- Get off of nightly through updating tiny-std to use global asm and 
reimplementing a bunch of missing symbols

## [v0.3.0] - 2022-12-06
### Fixed
 - Fullscreening causing crashing in some cases, because the destroyed window was cached and then reused

### Added
 - Generated new xcb to Rust code and used that instead
 - Replace stdlib with tiny-std

### Changed
 - Removed all libc dependencies, requiring nightly to run until [naked function stabilization](https://github.com/rust-lang/rust/pull/93587)
 - Using Dlmalloc as allocator 
 - Changed WM to be no_std, with the above change, another feature [that seems to be moving towards stabilization was added](https://github.com/rust-lang/rust/pull/102318)
 - Removed or patched dependencies needing libc or not being no_std compatible
 - Moved entrypoint to [pgwm](pgwm), 
moved the main WM from a binary project to a library project [pgwm-app](pgwm-app)
 - Changed configuration parsing of char-remap, now uses regular map-parsing


## [v0.2.0] - 2022-07-09

### Fixed
 - Splitting text on a char boundary caused a panic in some cases while using the tabbed mode
 - Correctly positions tiles and tab bar on monitors with a y-offset relative to the root screen
 - Status bar is redrawn in parts, further reducing CPU load

### Added
 - Reloading configuration without having to kill the WM
 - Event sourcing for debugging

### Changed
 - Fonts are now rasterized using [Fontdue](https://github.com/mooman219/fontdue).
 - Fonts are now drawn using xcb-xrender.
 - Font configuration changed, now font is not a String to a system font name
but a type containing a path to the specific font to be rendered and a pixel size to render it in. 
This is because libXft took care of that through fontconfig before, but now that dependency is gone.
 - No more unsafe code.
 - No c-library dependencies, can be built and statically linked, down to a ~2Mb binary with musl, and ~2Mb USS/PSS/RSS RAM footprint
 - Reworked the connection to be lighter and faster, eventual severe bugs in the implementation 
will cause a panic in debug and a freeze otherwise
 - Now exclusively uses Unix-sockets, no TCP is available

## [v0.1.0] - 2022-04-09
