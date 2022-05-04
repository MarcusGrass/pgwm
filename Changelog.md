# Change Log
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/)
and this project adheres to [Semantic Versioning](http://semver.org/).

## [Unreleased]

### Fixed

### Added
 - Reloading configuration without having to kill the WM

### Changed
 - Fonts are now rasterized using [Fontdue](https://github.com/mooman219/fontdue).
 - Fonts are now drawn using xcb-xrender.
 - Font configuration changed, now font is not a String to a system font name
but a type containing a path to the specific font to be rendered and a pixel size to render it in. 
This is because libXft took care of that through fontconfig before, but now that dependency is gone.
 - No more unsafe code.
 - No c-library dependencies, can be built and statically linked, down to a ~2Mb binary with musl, and ~2Mb USS/PSS/RSS RAM footprint

## [v0.1.0] - 2022-04-09
