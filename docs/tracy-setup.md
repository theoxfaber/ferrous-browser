# Tracy Setup

`ferrous-browser` uses `tracing-tracy = 0.11`, which maps to the upstream
Tracy profiler GUI `v0.11.1`. Use that GUI version so the wire format matches.

## Build the profiler GUI

```sh
git clone --depth 1 --branch v0.11.1 https://github.com/wolfpld/tracy /tmp/tracy-v0.11.1
cmake -S /tmp/tracy-v0.11.1/profiler -B /tmp/tracy-v0.11.1/profiler/build -G Ninja -DLEGACY=ON -DCMAKE_BUILD_TYPE=Release
cmake --build /tmp/tracy-v0.11.1/profiler/build --config Release -j 8
```

That produces:

```sh
/tmp/tracy-v0.11.1/profiler/build/tracy-profiler
```

`LEGACY=ON` is the safe choice for an X11 desktop. On Wayland-capable systems
you can usually omit it.

## Run the GUI and connect

In one terminal:

```sh
/tmp/tracy-v0.11.1/profiler/build/tracy-profiler
```

In another terminal, from the repo root:

```sh
cargo run --release --features tracy --example profile_run
```

The example will auto-connect to the Tracy GUI and also write
`ferrous-trace.json` for Perfetto / Chrome trace inspection.

## Pin the Chrome binary

If you want profiling runs to use the same Chrome for Testing binary as the
benchmark harnesses, set:

```sh
export CHROME_PATH="$HOME/.cache/puppeteer/chrome/linux-131.0.6778.204/chrome-linux64/chrome"
```

`Browser::launch_chrome()` now honors both `CHROME_PATH` and
`BrowserConfig.chrome_path`.
