# Migration Analysis: Python/Bash → Rust + Slint XXMapper

This document analyzes the existing controller-mapping implementation before
porting it to a native Rust + Slint GUI application.

## 1. Current architecture

The repository contains three reference implementations plus two example
configurations:

| File | Role |
| --- | --- |
| `map-conf-generator.py` | Interactive CLI mapper. Discovers a controller, captures each physical input, writes a `.conf` mapping. |
| `mohamed-xbox-controller.py` | Loads a `.conf` mapping, translates it into `xboxdrv` arguments and runs `xboxdrv` to create a virtual Xbox controller. |
| `samer-xbox-controller.sh` | Hard-coded for the DragonRise `0079:0006` controller: finds the evdev node with `udevadm`, builds `xboxdrv` arguments (including a Y-axis inversion) and runs `xboxdrv`. |
| `mohamed.conf` / `samer.conf` | Example mappings written by `map-conf-generator.py`. |

The pipeline is:

```
physical controller
        ↓
   /dev/input/by-id/*-event-joystick
        ↓
   map-conf-generator.py  (interactive capture → .conf)
        ↓
   mohamed-xbox-controller.py / samer-xbox-controller.sh
        ↓
        xboxdrv
        ↓
   virtual Xbox 360 controller (uinput)
```

There is no GUI. Configuration management, controller selection, and the
mapping process are all terminal driven.

## 2. Current event model

All event handling is done by reading raw Linux `input_event` structures
(`llHHI`) directly from the evdev file descriptor.

- `EV_KEY = 1`, `EV_ABS = 3`, `EV_SYN = 0`.
- Reads are non-blocking; `select(2)` is used to wait with a timeout.
- Stale events are drained with a zero-timeout `select` loop.
- `EAGAIN` / `BlockingIOError` is treated as "no event yet".

Recognized inputs:

- Button keys: `BTN_TRIGGER` (288) … `BTN_BASE6` (299), `BTN_SOUTH` (304),
  `BTN_EAST` (305), `BTN_NORTH` (307), `BTN_WEST` (308), `BTN_TL` (310),
  `BTN_TR` (311), `BTN_TL2` (312), `BTN_TR2` (313), `BTN_SELECT` (314),
  `BTN_START` (315), `BTN_MODE` (316), `BTN_THUMBL` (317), `BTN_THUMBR` (318).
- Absolute axes: `ABS_X` (0), `ABS_Y` (1), `ABS_Z` (2), `ABS_RX` (3),
  `ABS_RY` (4), `ABS_RZ` (5).
- Hat/d-pad axes: `ABS_HAT0X` (16), `ABS_HAT0Y` (17), plus HAT1…HAT4.

### Center / dead-zone detection

Axes are considered "at rest" when the value is inside `[100, 155]`. This is a
hard-coded heuristic tuned for the DragonRise `0..255` axis range. A value
outside that window counts as movement; a value inside it counts as release.

## 3. Current mapping model

The `.conf` format is one line per Xbox control:

```
name=type,code,value,event_name
```

Examples:

```
left_stick_x=abs,0,80,ABS_X
r2=key,313,1,KEY_313
dpad_left=abs,16,-1,ABS_HAT0X
guide=disabled
```

The generator captures 21 controls in a fixed order:

```
left_stick_x, left_stick_y, right_stick_x, right_stick_y   (stick axes)
l2, r2                                                      (triggers)
dpad_left, dpad_right, dpad_up, dpad_down                  (d-pad)
a, b, x, y                                                  (face buttons)
l1, r1                                                      (bumpers)
back, start, guide                                          (menu buttons)
l3, r3                                                      (stick clicks)
```

The model is a loose collection of strings. `mohamed-xbox-controller.py`
normalizes legacy `KEY_304` style names into `BTN_SOUTH` style names and
aliases `lb/l1`, `rb/r1`, `lt/l2`, `rt/r2`, `tl/l3`, `tr/r3`.

### Release detection

For every control the mapper:

1. drains stale events,
2. waits for the qualifying press/move event,
3. waits until the physical input is released (button `EV_KEY value=0`, axis
   back to center, hat back to `0`),
4. only then advances to the next control.

A `RELEASE_TIMEOUT` of 2 seconds bounds the release wait.

## 4. Current Xbox backend

Both launch scripts eventually execute:

```
xboxdrv --evdev /dev/input/eventN --mimic-xpad \
        --evdev-absmap ABS_X=x1,ABS_Y=y1,...,ABS_HAT0X=dpad_x,ABS_HAT0Y=dpad_y \
        --evdev-keymap BTN_SOUTH=a,BTN_EAST=b,...,BTN_THUMBL=tl,BTN_THUMBR=tr \
        [--axismap -Y1=Y1,-Y2=Y2]
```

- `--mimic-xpad` makes xboxdrv expose a virtual Xbox 360 pad.
- `--evdev-absmap` maps physical axes to virtual Xbox axes (`x1`, `y1`, `x2`,
  `y2` for the sticks, `lt`/`rt` for triggers, `dpad_x`/`dpad_y` for the
  d-pad).
- `--evdev-keymap` maps physical buttons to virtual Xbox buttons (`a`, `b`,
  `x`, `y`, `lb`, `rb`, `tl`, `tr`, `back`, `start`, `guide`).
- `--axismap -Y1=Y1,-Y2=Y2` inverts both stick Y axes (output-side
  correction). This is only applied by `samer-xbox-controller.sh`; it is NOT
  applied by `mohamed-xbox-controller.py`.

## 5. Current configuration model

- Configuration lives in a single `.conf` file next to the scripts.
- There is no controller identity. The DragonRise case is hard-coded by
  `VID:PID` (`0079:0006`); `mohamed-xbox-controller.py` picks whichever
  joystick is found first.
- No persistence directory, no versioning, no per-controller settings.

## 6. Known bugs

1. **Y-axis inversion bug.** `mohamed-xbox-controller.py` does not apply the
   `--axismap -Y1=Y1,-Y2=Y2` correction that the Bash script uses, so physical
   UP can become Xbox DOWN while LEFT/RIGHT work correctly. Even where the
   correction exists, it is hard-coded for the DragonRise and not derived from
   the actual captured event direction.
2. **No stable identity.** Controllers are matched by a hard-coded `VID:PID`
   or by "first joystick found". Two identical controllers (same VID/PID/name)
   cannot be configured independently, and `eventN` numbers are transient.
3. **Hard-coded center range.** `[100, 155]` is wrong for `-32768..32767`
   devices and for triggers whose rest position differs.
4. **ENTER-based skip ambiguity.** The mapper waits for ENTER to skip, but the
   first control required "Press ENTER to begin", and stale ENTER presses leak
   into the loop.
5. **No GUI / no reconnect.** The user must run commands manually, and there is
   no automatic handling of plug/unplug.
6. **String-typed mapping.** The core mapping is built from ad-hoc strings,
   making validation, editing and testing fragile.

## 7. Proposed Rust architecture

```
src/
    main.rs                     # entry point, wires everything together
    app/
        mod.rs
        state.rs                # UI-agnostic application state
    controllers/
        mod.rs
        discovery.rs            # scan /dev/input + udev properties
        identity.rs             # stable per-controller identity + id()
        evdev.rs                # event reader abstraction + evdev impl
    mapping/
        mod.rs
        model.rs                # typed ControllerMapping / InputSource
        detector.rs             # capture state machine (press→release)
        mapper.rs               # runtime translation physical→Xbox
        layouts.rs              # Custom / PS3 / PS4
    xbox/
        mod.rs
        emulator.rs             # Xbox backend trait + xboxdrv/uinput impls
    config/
        mod.rs
        model.rs                # JSON config schema (versioned)
        storage.rs              # XDG config dir + atomic writes
    ui/
        mod.rs                  # Slint glue
ui/
    main.slint
    controller_list.slint
    mapping_view.slint
    controller_settings.slint
```

Design decisions:

- **Controller identity.** Built from `ID_VENDOR_ID`, `ID_MODEL_ID`, serial
  (`ID_SERIAL_SHORT` when available) and stable udev path (`ID_PATH` /
  `ID_PATH_TAG`). A deterministic, sanitized `controller_id` is derived from
  serial when present, otherwise from the physical path. The transient
  `/dev/input/eventN` node is never used as an identity.
- **Typed mapping model.** `InputSource` (`Key` / `Axis` / `Hat`) and
  `AxisMapping { source, invert }` replace the `name=type,code,value,name`
  strings. Axes carry an explicit `invert` flag.
- **Self-correcting axis direction.** During mapping the generator instructs
  LEFT/UP, captures the raw sign, and sets `invert` so the captured physical
  direction maps to the Xbox negative (LEFT/UP) direction. This fixes the
  Y-inversion bug generically instead of hard-coding `-Y1=Y1`.
- **Event abstraction.** A trait (or equivalent) around "read next event /
  drain" lets the detector and mapper be unit-tested with synthetic events;
  the real evdev implementation uses `poll(2)` so the GUI stays responsive.
- **Xbox backend.** A backend trait with two implementations: `xboxdrv`
  (generated from the typed mapping, with `--device-name` for custom virtual
  names) and a pure-Rust `uinput` virtual Xbox pad. Backend quirks stay inside
  `src/xbox/`.
- **Configuration.** Single versioned `config.json` under
  `~/.local/share/XXMapper/`, written atomically (temp file + rename). The
  format is designed to be migrated via a `version` field.
- **Background threads.** evdev reading, monitoring, mapping capture and Xbox
  backends all run off the UI thread; the Slint UI communicates over channels
  and remains responsive.
- **Multiple controllers.** Each controller gets its own identity, its own
  mapping and its own backend instance.