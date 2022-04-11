# PGWM, a DWM-inspired tiling window manager written in Rust
[![Latest workflow](https://github.com/MarcusGrass/pgwm/workflows/CI/badge.svg)](https://github.com/MarcusGrass/pgwm/actions)

The WM is to my understanding compliant with the [wm-spec](https://specifications.freedesktop.org/wm-spec/wm-spec-1.3.html).
The specification is fairly difficult to understand (for me) and for a tiling window manager some sometimes liberal
interpretations has to be made. If something seems implemented wrongly, and it isn't covered by
an explanation in [EWMH.md](docs/EWMH.md), please create an issue if you have the time.
Big shout out to [x11rb](https://github.com/psychon/x11rb) which has great safe x11 bindings!

# Why
I love to build my old tools, so after a few years of running up against bugs that bothered my workflow 
and features that I wanted but were missing in other tiling WMs
I decided to build a WM that does exactly what I want it to do. I considered hacking dwm, but I'm not that hot with 
`C` and decided against it, opting to write it in Rust instead. 

# Primary goals
A WM that is fast, has low resource-usage, is feature complete according to my needs,
doesn't contain nasty bugs or QoL-detriments, and is ewmh-compliant.

# Implemented features
- Crashes on errors (yes this is a feature)
- It can tile windows in a left-leader or center-leader pattern
- It can display windows tabbed with the window title on top
- Windows are configured into workspaces
- Workspaces can be moved between monitors
- Workspace-layouts can be cycled (left-leader -> center-leader -> tabbed)
- Windows within workspaces can have their order rearranged
- Windows can be moved between workspaces
- When a window is moved from a workspace it ends up at the first tab-order
- Windows can be manually floated
- Floating windows can be dragged
- Windows can be manually un-floated
- It hosts a bar at the top of each monitor
- Sections of the bar can have their on-click behaviour customized
- The bar can be hidden
- The displays workspace information, window title name, status-checks, and shortcuts
- Windows can be resized
- Visible cursor without window supplying it
- Most of the window-manipulation that the WM does can be configured
- Keys (or clickable sections of the bar) can be configured to run arbitrary processes (some custom bash for example)
- Windows can be mapped to workspaces by their `WM_CLASS` property

# Comparison to other WMs
I'm only comparing other WMs which I have used extensively hopefully I can extend this to include more than 2 other WMs.

## DWM
The closest WM in terms of feature-parity would be DWM, although this WM tries to do a bit more by default than DWM does.
A major difference between this WM and DWM is that instead of using tags with
different tags per monitor this WM operates closer to DWM's [single tagset patch](https://dwm.suckless.org/patches/single_tagset/).

## Xmonad
The beauty of xmonad and using haskel for a configurable WM is that through a functional language Xmonad provides
an absurd level of customizability, this lends itself well to contributors creating additions through xmonad-contrib.
This WM does not aim to match that level of customizability but instead tries to be minimal and reasonably configurable,
while also being distributable and configurable through a binary rather than compiled-configuration.

# Usage
Some getting-started information is described [here](docs/USAGE.md).

# Future plans
As far as features go the plan is not to add complicated features, but simple flexible things. The WM is now 
according to my needs feature complete. However, I most likely don't know what I'm missing, if there's a feature 
that you'd like to see I'd love to hear about it and will consider adding it, but I am fairly conservative when it 
comes to adding features that I wouldn't use, the best place for those would be a fork.
When it comes to ergonomics and documentation I'm very open to add things. One big thing is the configuration,
it's a simplest possible mapping, almost 1 to 1 to internal datastructures which makes it cumbersome.
Instead of configuring a status check in one place, and then a mouse-mapping on click for it in another 
something more reasonable might be to configure a status check and in the same place some on-click action, things like that.
I've been hesitant about working too much on configuration ergonomics since I don't know if there is any demand for that, 
if there is, please let me know by creating an issue.

# How it looks
### Default config, tabbed on the left, left-leader-layout on the right.
![multi-monitor-tiled](demo1.png)
### Default config, left-leader-layout on the left, a floating window on the right, above the left-leader-layout
![multi-monitor-tabbed-float](demo2.png)
### Default config, single monitor center-leader
![multi-monitor-tabbed-float](demo3.png)

# How to build
To build locally libx11-dev, libxft-dev, platform build essentials, and lld is required
see the [min building dockerfile](.docker/minimal-build.dockerfile).  
Lld is not a strict requirement but if not using lld either edit or remove the line `"-C", "link-arg=-fuse-ld=lld",` from
[config.toml](.cargo/config.toml)  
To run the same test as the ci locally libssl and perl is also required, 
[see the ci dockerfile](.docker/full-checks.dockerfile).


The project is tested on x86_64-unknown-linux-gnu but "should" run on any *nix system. 

## Install a Rust toolchain
https://www.rust-lang.org/tools/install

## Clone this repo
git clone https://github.com/MarcusGrass/pgwm.git

## Build the project
The project builds default with xinerama support, a status-bar, and support for a config-file. To compile without either,
disable default features.
To build with max optimizations use --profile=optimized.
In [config.toml](.cargo/config.toml) --release is set to compile with debug assertions, usually when I'm developing 
the WM I run it like that to ensure that there are no overflows/underflows, x11 uses i16s, u16s, i32s, and u32s fairly interchangeably 
which poses a conversion risk. Removing that options will yield a negligible performance increase if compiling --release.    
In benchmarking, heavier calculations see a speedup of around 15-45% on optimized compared to release on my machine, 
that being said we're talking about 190 to 150 nanoseconds for calculating tiling positions, there aren't many heavy calculations
being performed, most latency is from x11 redrawing windows.  
The project can also be compiled with debug output, the binary will then output various debug info to stderr.

### Using make
There's a [configure script](configure) for convenience, although it's basically just
a silly posix-shell script that generates a Makefile.

If no binary directory is supplied, the script will try to install to `$HOME/.local/bin`, 
if the environment variable `$HOME` is not set and no binary directory is supplied the script will fail.  
if your path variable doesn't include that directory you can export that for example in your `~/.bashrc` 
with: `export PATH=$HOME/.local/bin:$PATH`, if you want the wm to be launchable by just `pgwm`.  

If no configuration directory is supplied, the script will try to install configuration to `$XDG_CONFIG_HOME/pgwm`, 
if the environment variable `$XDG_CONFIG_HOME` is not set, it will check if the environment variable `$HOME` is set and 
try to install configuration in `$HOME/.config/pgwm`. If neither `$XDG_CONFIG_HOME` nor `$HOME` is set, and no 
configuration directory was supplied, the script will fail.

  
Build default:  
```shell
./configure
make
make install
# Cleans up build directory
make clean
```
Use `./configure --help` to see options, by default uninstall does not remove the configuration file, the full lifecycle from nothing
to a clean uninstall would be
```shell
./configure
make
make install
make clean
# Uninstall and remove config
make uninstall CLEAN_CONFIG=1
```

### Manual build
How to build as a regular Rust project.

#### With default features
`cargo build --release`
or
`cargo build --profile=optimized`

#### With no default features
`cargo build --release --no-default-features`
or
`cargo build --profile=optimized --no-default-features`

#### Example of some additional features
`cargo build --release --no-default-features --features xinerama,status-bar`  
or  
`cargo build --profile=optimized --no-default-features --features xinerama,status-bar`

#### Using cargo install
Installs the binary to `$HOME/.cargo/bin`  
`cargo install --profile=optimized --path pgwm`  
Remember to add cargo bin to path if you haven't already  
`PATH="$HOME/.cargo/bin:$PATH"`

### Edit .xinitrc or other file specifying WM entrypoint
If built with `cargo build` The binary ends up in target/release/pgwm or target/optimized/pgwm
Replace the (probably) last line of .xinitrc with
`exec $BINARY_LOCATION` $BINARY_LOCATION being the path to the pgwm binary, or just `pgwm` if using `cargo install`.   

If you want it to be hot-reloadable, say if you make a code or config-change (and recomopile in the case of code-changes) and quit (program exits 0), it'll start back up immediately
without open applications closing, replace the last line of .xinitrc with
`BINARY_LOCATION="$SOURCE_DIR/target/release/pgwm"`
`while type $BINARY_LOCATION >/dev/null ; do $BINARY_LOCATION && continue || break ; done`.
In that case, exiting would require killing the wm some other way, like `killall pgwm`.

# Changing configuration
## Config file
The WM can be configured by either using a configuration file, a sample configuration exists [here in the repo](pgwm.toml) 
the different properties are commented and hopefully makes sense. The file needs to be placed at `$XDG_CONFIG_HOME/pgwm/pgwm.toml`
or if `$XDG_CONFIG_HOME` is not set, `$HOME/.config/pgwm/pgwm.toml`. If none of the environment variables `$HOME` or `$XDG_CONFIG_HOME` are set, 
or if the file does not exist, the WM will use hard-coded configuration.  
Constants that need to be known at compile time for stack-usage reasons are hard-coded and described further in the below section.

## Hard coded configuration
Hard coded configuration resides in [pgwm_core/src/config/mod.rs](pgwm-core/src/config/mod.rs) and consists of rust code.
The configuration is mostly constants with some functions, some constants are limits, such as `WS_WINDOW_LIMIT`,
the reason for it existing is that a lot of heapless datastructures are used.
If you were to set the `WS_WINDOW_LIMIT` to 2, and try to spawn 3 windows on a workspace, the application would crash.
A rule of thumb for the error-handling is that all errors which are unexpected immediately causes a crash, and every crash
signifies a misconfiguration or programming error. The reason for keeping it this way is so that bugs doesn't go by silently.
If something causes a crash I want to fix the issue rather than have the application limp along. 
If you decide to try this WM out and find a bug, please report it as an issue in this repo or make a PR if you have the time.  

Note: A crash here is a rust `panic`, the WM should never segfault, regardless of misconfiguration. If it does please 
file an issue.

# Easy mistakes to make
There are a few easy mistakes to make that will make the WM run strangely. 
- The configured fonts do not exist on the machine. The default configuration of this WM is one that I use, and if you do not
have the same fonts as I do the results will be weird. Update the default configuration to use a font that exists on the machine.
`fc-list` will find available fonts.
- Misconfiguration causing startup issues, if the issues start after increasing something, more fonts, more keybinds etc.
Then most likely a hardcoded limit has been reached, either create an Issue to increase the limit, recompile it yourself with a higher limit, 
or make a PR with the limit increased. Limits are found [with the hardcoded configuration](pgwm-core/src/config/mod.rs).
- Fewer defined workspaces than amount of monitors will result in unused monitors
- An invalid config will cause a crash at startup while a missing config won't.
The reason for this is that you might want to use default configurations and skip
having a config file even if compiled with the `config-file` feature. 
On the other hand, if you make a configuration mistake then finding that out immediately 
is in my opinion less confusing than suddenly getting the default configuration.
- If running some applications built on java frameworks, such as Jetbrains IDE's, 
putting the below lines in your ~/.xinitr may be required for them to work properly.
```Bash
export _JAVA_AWT_WM_NONREPARENTING=1
export AWT_TOOLKIT=MToolkit
wmname compiz # or wmname LG3D
```

# Resources
The WM, according to `smem` on glibc, has a PSS of around 5Mb Memory usage and no known memory leaks,
a memory leak would likely result in a crash rather than a slow increase over time since most if not all dynamic
datastructures are on the stack.  
CPU-wise it has a fairly low usage, with idle usage that depends completely on whether you use the status bar, what update frequency you're
running it on, and which checks you're using. Running the full checks on a 1 second interval as I am doing currently draws very little cpu.

Some update-intensive operations will cause cpu-spikes, such as dragging a floating window. For each motion-event coming from x11
the WM reconfigures the window. The same thing will occur if resizing tiled windows, each resize will cause a reconfiguration
of windows in that tile-set.

Some programs like Jetbrains IDEs update the WM name on every keystroke, depending on
typing speed this may result in a lot of events and workspace bar redraws. However, that operation is so CPU-efficient that it isn't worth
making an effort to reduce workspace bar redraws on name-changes by caching or otherwise.

Some examples to get a feel for the resource usage (the WM runs single threaded):  
At idle with no status bar I get a 0% single-core CPU usage, with status bar it spikes about once a second to 0.6%.  
An extremely violent window drag results in an at most 10% single-core CPU usage.  
Furious typing into a Jetbrains IDE gets at most 1.3% single-core CPU usage.

All this being said, it's measured for the running WM binary, all operations on the x11-server will cause some overhead there,
this WM binary could be perfectly efficient but slamming the x11 server with requests that it has problems processing.
Although I have not noticed any such behaviour.

# Licensing
This project is licensed under [GPL v3](LICENSE).
