# PGWM, a DWM-inspired tiling window manager written in Rust
[![Latest workflow](https://github.com/MarcusGrass/pgwm/workflows/CI/badge.svg)](https://github.com/MarcusGrass/pgwm/actions)

The WM is to my understanding compliant with the [wm-spec](https://specifications.freedesktop.org/wm-spec/wm-spec-1.3.html).
The specification is fairly difficult to understand (for me) and for a tiling window manager some sometimes liberal
interpretations has to be made. If something seems implemented wrongly, and it isn't covered by
an explanation in [EWMH.md](EWMH.md), please create an issue if you have the time.
Big shout out to [x11rb](https://github.com/psychon/x11rb) which has great safe 11 bindings!

# Why
I love to build my old tools, so after a few years of running up against bugs that bothered my workflow and features that I wanted but were missing in other tiling WMs
I decided to build a WM that does exactly what I want it to do. I considered hacking dwm, but I'm not that hot with 
`C` and decided against it, opting to rewrite it in Rust instead. 

# Primary goals
A WM that is fast, has low resource-usage, is feature complete according to my needs, doesn't contain nasty bugs or QoL-detriments, and is ewmh-compliant.  

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

# How it looks
### Default config, tabbed on the left, left-leader-layout on the right.
![multi-monitor-tiled](demo1.png)
### Default config, left-leader-layout on the left, a floating window on the right, above the left-leader-layout
![multi-monitor-tabbed-float](demo2.png)
### Default config, single monitor center-leader
![multi-monitor-tabbed-float](demo3.png)

# How to build
To build locally libx11-dev, libxft-dev, platform build essentials, and lld is required
see the [min building dockerfile](minimal-build.dockerfile).  
Lld is not a strict requirement but if not using lld either edit or remove the line `"-C", "link-arg=-fuse-ld=lld",` from
[config.toml](.cargo/config.toml)  
To run the same test as the ci locally libssl and perl is also required, 
[see the ci dockerfile](.github/Dockerfile).


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

### With default features
`cargo build --release`
or
`cargo build --profile=optimized`

### With no default features
`cargo build --release --no-default-features`
or
`cargo build --profile=optimized --no-default-features`

### Example of some additional features
`cargo build --release --no-default-features --features xinerama,status-bar`
or
`cargo build --profile=optimized --no-default-features --features xinerama,status-bar`

## Directly installing
Installing the binary to $HOME/.cargo/bin  
`cargo install --profile=optimized --path pgwm`  
Remember to add cargo bin to path if you haven't already  
`PATH="$HOME/.cargo/bin:$PATH"`

## Edit .xinitrc or other file specifying WM entrypoint
If built with `cago build` The binary ends up in target/release/pgwm or target/optimized/pgwm
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

# Licensing
This project is licensed under [GPL v3](LICENSE).
