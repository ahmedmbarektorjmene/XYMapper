#!/usr/bin/env python3

import glob
import os
import select
import struct
import sys
import termios
import tty
import time


# ============================================================
# Linux input definitions
# ============================================================

EVENT_STRUCT = struct.Struct("llHHI")

EV_SYN = 0
EV_KEY = 1
EV_ABS = 3

# Physical joystick buttons exposed by many DragonRise devices.
BTN_NAMES = {
    288: "BTN_TRIGGER",
    289: "BTN_THUMB",
    290: "BTN_THUMB2",
    291: "BTN_TOP",
    292: "BTN_TOP2",
    293: "BTN_PINKIE",
    294: "BTN_BASE",
    295: "BTN_BASE2",
    296: "BTN_BASE3",
    297: "BTN_BASE4",
    298: "BTN_BASE5",
    299: "BTN_BASE6",
}

ABS_NAMES = {
    0: "ABS_X",
    1: "ABS_Y",
    2: "ABS_Z",
    3: "ABS_RX",
    4: "ABS_RY",
    5: "ABS_RZ",
    16: "ABS_HAT0X",
    17: "ABS_HAT0Y",
}

AXIS_CODES = {0, 1, 2, 3, 4, 5}
DPAD_CODES = {16, 17}

CENTER_MIN = 100
CENTER_MAX = 155

# How long we wait after detecting an action before considering
# the release phase complete.
RELEASE_TIMEOUT = 2.0

# Small delay after opening the controller, allowing stale events
# to settle.
STARTUP_DRAIN_TIME = 0.15


# ============================================================
# Xbox mapping
# ============================================================

CONTROLS = [
    {
        "name": "left_stick_x",
        "display": "LEFT STICK ←",
        "instruction": "Move the LEFT STICK fully LEFT, then RELEASE it.",
        "kind": "axis",
    },
    {
        "name": "left_stick_y",
        "display": "LEFT STICK ↑",
        "instruction": "Move the LEFT STICK fully UP, then RELEASE it.",
        "kind": "axis",
    },
    {
        "name": "right_stick_x",
        "display": "RIGHT STICK ←",
        "instruction": "Move the RIGHT STICK fully LEFT, then RELEASE it.",
        "kind": "axis",
    },
    {
        "name": "right_stick_y",
        "display": "RIGHT STICK ↑",
        "instruction": "Move the RIGHT STICK fully UP, then RELEASE it.",
        "kind": "axis",
    },

    {
        "name": "l2",
        "display": "L2 — LEFT TRIGGER",
        "instruction": "Press L2 fully, then RELEASE it.",
        "kind": "trigger",
    },
    {
        "name": "r2",
        "display": "R2 — RIGHT TRIGGER",
        "instruction": "Press R2 fully, then RELEASE it.",
        "kind": "trigger",
    },

    {
        "name": "dpad_left",
        "display": "D-PAD ←",
        "instruction": "Press D-PAD LEFT, then RELEASE it.",
        "kind": "dpad",
    },
    {
        "name": "dpad_right",
        "display": "D-PAD →",
        "instruction": "Press D-PAD RIGHT, then RELEASE it.",
        "kind": "dpad",
    },
    {
        "name": "dpad_up",
        "display": "D-PAD ↑",
        "instruction": "Press D-PAD UP, then RELEASE it.",
        "kind": "dpad",
    },
    {
        "name": "dpad_down",
        "display": "D-PAD ↓",
        "instruction": "Press D-PAD DOWN, then RELEASE it.",
        "kind": "dpad",
    },

    {
        "name": "a",
        "display": "A",
        "instruction": "Press A, then RELEASE it.",
        "kind": "button",
    },
    {
        "name": "b",
        "display": "B",
        "instruction": "Press B, then RELEASE it.",
        "kind": "button",
    },
    {
        "name": "x",
        "display": "X",
        "instruction": "Press X, then RELEASE it.",
        "kind": "button",
    },
    {
        "name": "y",
        "display": "Y",
        "instruction": "Press Y, then RELEASE it.",
        "kind": "button",
    },

    {
        "name": "l1",
        "display": "L1 — LEFT BUMPER",
        "instruction": "Press L1, then RELEASE it.",
        "kind": "button",
    },
    {
        "name": "r1",
        "display": "R1 — RIGHT BUMPER",
        "instruction": "Press R1, then RELEASE it.",
        "kind": "button",
    },

    {
        "name": "back",
        "display": "BACK / SELECT",
        "instruction": "Press BACK / SELECT, then RELEASE it.",
        "kind": "button",
    },
    {
        "name": "start",
        "display": "START",
        "instruction": "Press START, then RELEASE it.",
        "kind": "button",
    },
    {
        "name": "guide",
        "display": "GUIDE / HOME",
        "instruction": "Press GUIDE / HOME if your controller has one, then RELEASE it.",
        "kind": "button",
    },

    {
        "name": "l3",
        "display": "L3 — LEFT STICK CLICK",
        "instruction": "CLICK the LEFT STICK, then RELEASE it.",
        "kind": "button",
    },
    {
        "name": "r3",
        "display": "R3 — RIGHT STICK CLICK",
        "instruction": "CLICK the RIGHT STICK, then RELEASE it.",
        "kind": "button",
    },
]


# ============================================================
# Terminal handling
# ============================================================

class RawTerminal:
    def __enter__(self):
        self.fd = sys.stdin.fileno()
        self.old = termios.tcgetattr(self.fd)
        tty.setcbreak(self.fd)
        return self

    def __exit__(self, exc_type, exc_value, traceback):
        termios.tcsetattr(
            self.fd,
            termios.TCSADRAIN,
            self.old,
        )


def enter_pressed():
    """
    Non-blocking check for ENTER.

    We use cbreak mode so ENTER is immediately visible without
    requiring another prompt/input() call.
    """
    ready, _, _ = select.select(
        [sys.stdin],
        [],
        [],
        0,
    )

    if not ready:
        return False

    try:
        data = os.read(
            sys.stdin.fileno(),
            64,
        )
    except OSError:
        return False

    return b"\n" in data or b"\r" in data


# ============================================================
# evdev helpers
# ============================================================

def event_name(event_type, code):
    if event_type == EV_KEY:
        return BTN_NAMES.get(
            code,
            f"KEY_{code}",
        )

    if event_type == EV_ABS:
        return ABS_NAMES.get(
            code,
            f"ABS_{code}",
        )

    return f"TYPE_{event_type}_CODE_{code}"


def read_event(fd):
    """
    Read one Linux input_event.

    IMPORTANT:
    The fd is non-blocking, so EAGAIN/EWOULDBLOCK is normal.
    """
    try:
        data = os.read(
            fd,
            EVENT_STRUCT.size,
        )
    except BlockingIOError:
        return None
    except OSError:
        return None

    if len(data) != EVENT_STRUCT.size:
        return None

    _, _, event_type, code, value = EVENT_STRUCT.unpack(data)

    return event_type, code, value


def drain_events(fd):
    """
    Remove all currently queued events.

    This is extremely important between mappings so that a release
    event from the previous action cannot become the next mapping.
    """
    while True:
        try:
            ready, _, _ = select.select(
                [fd],
                [],
                [],
                0,
            )
        except (OSError, ValueError):
            return

        if not ready:
            break

        event = read_event(fd)

        if event is None:
            break


def axis_is_center(value):
    return CENTER_MIN <= value <= CENTER_MAX


def axis_is_moved(value):
    return not axis_is_center(value)


# ============================================================
# Waiting for events
# ============================================================

def wait_for_event(fd, timeout=0.05):
    """
    Wait for either a controller event or timeout.

    Returns:
        event
        None
    """
    ready, _, _ = select.select(
        [fd],
        [],
        [],
        timeout,
    )

    if not ready:
        return None

    return read_event(fd)


def wait_until_released_button(fd, code):
    """
    Wait until a button generates its release event.
    """
    deadline = time.monotonic() + RELEASE_TIMEOUT

    while time.monotonic() < deadline:
        event = wait_for_event(fd)

        if event is None:
            continue

        event_type, event_code, value = event

        if event_type != EV_KEY:
            continue

        if event_code != code:
            continue

        # Linux EV_KEY:
        # 0 = release
        # 1 = press
        # 2 = autorepeat
        if value == 0:
            return True

    return False


def wait_until_axis_released(fd, code):
    """
    Wait until the axis returns close enough to its center.
    """
    deadline = time.monotonic() + RELEASE_TIMEOUT

    while time.monotonic() < deadline:
        event = wait_for_event(fd)

        if event is None:
            continue

        event_type, event_code, value = event

        if event_type != EV_ABS:
            continue

        if event_code != code:
            continue

        if axis_is_center(value):
            return True

    return False


def wait_until_dpad_released(fd, code):
    """
    D-pad returns to zero after release.
    """
    deadline = time.monotonic() + RELEASE_TIMEOUT

    while time.monotonic() < deadline:
        event = wait_for_event(fd)

        if event is None:
            continue

        event_type, event_code, value = event

        if event_type != EV_ABS:
            continue

        if event_code != code:
            continue

        if value == 0:
            return True

    return False


# ============================================================
# Detection
# ============================================================

def detect_axis(fd):
    """
    Detect an axis movement.

    We DO NOT return immediately after seeing the first event.

    We first detect the axis, then wait for the axis to return
    to its center.
    """
    while True:
        event = wait_for_event(fd)

        if event is None:
            continue

        event_type, code, value = event

        if event_type != EV_ABS:
            continue

        if code not in AXIS_CODES:
            continue

        if not axis_is_moved(value):
            continue

        name = event_name(
            event_type,
            code,
        )

        initial_value = value

        # Now wait for release.
        wait_until_axis_released(
            fd,
            code,
        )

        return {
            "type": "abs",
            "code": code,
            "value": initial_value,
            "name": name,
        }


def detect_trigger(fd):
    """
    Triggers can be reported either as an ABS axis or as a KEY
    on cheap/old controllers.

    Accept either.
    """
    while True:
        event = wait_for_event(fd)

        if event is None:
            continue

        event_type, code, value = event

        # Trigger exposed as an analog axis.
        if event_type == EV_ABS and code in AXIS_CODES:
            if not axis_is_moved(value):
                continue

            name = event_name(
                event_type,
                code,
            )

            initial_value = value

            wait_until_axis_released(
                fd,
                code,
            )

            return {
                "type": "abs",
                "code": code,
                "value": initial_value,
                "name": name,
            }

        # Trigger exposed as a button.
        if event_type == EV_KEY and value == 1:
            name = event_name(
                event_type,
                code,
            )

            wait_until_released_button(
                fd,
                code,
            )

            return {
                "type": "key",
                "code": code,
                "value": 1,
                "name": name,
            }


def detect_button(fd):
    """
    Detect button press, then wait for release.
    """
    while True:
        event = wait_for_event(fd)

        if event is None:
            continue

        event_type, code, value = event

        if event_type != EV_KEY:
            continue

        if value != 1:
            continue

        name = event_name(
            event_type,
            code,
        )

        wait_until_released_button(
            fd,
            code,
        )

        return {
            "type": "key",
            "code": code,
            "value": 1,
            "name": name,
        }


def detect_dpad(fd, expected_direction):
    """
    Detect D-pad direction.

    expected_direction:

        dpad_left  -> ABS_HAT0X = -1
        dpad_right -> ABS_HAT0X = +1
        dpad_up    -> ABS_HAT0Y = -1
        dpad_down  -> ABS_HAT0Y = +1

    We only accept the requested direction.
    """
    if expected_direction == "dpad_left":
        expected_code = 16
        expected_value = -1

    elif expected_direction == "dpad_right":
        expected_code = 16
        expected_value = 1

    elif expected_direction == "dpad_up":
        expected_code = 17
        expected_value = -1

    elif expected_direction == "dpad_down":
        expected_code = 17
        expected_value = 1

    else:
        raise ValueError(
            f"Unknown D-pad direction: {expected_direction}"
        )

    while True:
        event = wait_for_event(fd)

        if event is None:
            continue

        event_type, code, value = event

        if event_type != EV_ABS:
            continue

        if code != expected_code:
            continue

        # evdev values should normally be -1/0/+1.
        # Some Python/platform combinations can expose unsigned
        # values for -1, so normalize it.
        if value == 0xFFFFFFFF:
            value = -1

        if value != expected_value:
            continue

        name = event_name(
            event_type,
            code,
        )

        wait_until_dpad_released(
            fd,
            code,
        )

        return {
            "type": "abs",
            "code": code,
            "value": expected_value,
            "name": name,
        }


# ============================================================
# Mapping controller
# ============================================================

def map_control(fd, control):
    kind = control["kind"]

    if kind == "axis":
        return detect_axis(fd)

    if kind == "trigger":
        return detect_trigger(fd)

    if kind == "button":
        return detect_button(fd)

    if kind == "dpad":
        return detect_dpad(
            fd,
            control["name"],
        )

    raise RuntimeError(
        f"Unknown control kind: {kind}"
    )


# ============================================================
# Controller discovery
# ============================================================

def find_controllers():
    """
    Find joystick event devices through /dev/input/by-id.

    We prefer *-event-joystick because that is the evdev interface
    we need.
    """
    paths = sorted(
        glob.glob(
            "/dev/input/by-id/*-event-joystick"
        )
    )

    controllers = []

    for path in paths:
        if not os.path.exists(path):
            continue

        try:
            real_path = os.path.realpath(path)

            if not real_path.startswith(
                "/dev/input/event"
            ):
                continue

            controllers.append(
                {
                    "name": os.path.basename(path),
                    "path": path,
                    "real_path": real_path,
                }
            )

        except OSError:
            continue

    return controllers


def choose_controller():
    controllers = find_controllers()

    if not controllers:
        print()
        print("ERROR: No joystick/controller devices found.")
        print()
        print("Check:")
        print("  ls -l /dev/input/by-id/")
        print()
        sys.exit(1)

    print()
    print("=" * 60)
    print(" CONTROLLER SELECTION")
    print("=" * 60)
    print()

    for index, controller in enumerate(
        controllers,
        1,
    ):
        print(
            f"  [{index}] "
            f"{controller['name']}"
        )
        print(
            f"      {controller['path']}"
        )
        print()

    while True:
        try:
            answer = input(
                f"Select controller [1-{len(controllers)}]: "
            ).strip()

            index = int(answer)

            if 1 <= index <= len(controllers):
                selected = controllers[index - 1]

                print()
                print("Selected:")
                print(
                    f"  {selected['path']}"
                )
                print()

                return selected["path"]

        except (ValueError, EOFError):
            pass

        print(
            f"Please enter a number from "
            f"1 to {len(controllers)}."
        )


# ============================================================
# Configuration
# ============================================================

def write_config(
    output,
    mappings,
):
    with open(
        output,
        "w",
        encoding="utf-8",
    ) as f:
        f.write(
            "# DragonRise -> Xbox 360 mapping\n"
        )
        f.write(
            "# Generated automatically.\n"
        )
        f.write(
            "#\n"
        )
        f.write(
            "# Format:\n"
        )
        f.write(
            "# name=type,code,value,event_name\n"
        )
        f.write(
            "#\n"
        )

        for control in CONTROLS:
            name = control["name"]

            if name not in mappings:
                f.write(
                    f"{name}=disabled\n"
                )
                continue

            mapping = mappings[name]

            f.write(
                f"{name}="
                f"{mapping['type']},"
                f"{mapping['code']},"
                f"{mapping['value']},"
                f"{mapping['name']}\n"
            )


# ============================================================
# Duplicate checking
# ============================================================

def mapping_source(mapping):
    """
    Return the physical source identity.

    D-pad directions intentionally share an ABS code, but have
    different values, so value is included.
    """
    return (
        mapping["type"],
        mapping["code"],
        mapping["value"],
    )


def find_duplicate(
    mappings,
    mapping,
):
    source = mapping_source(mapping)

    for name, existing in mappings.items():
        if mapping_source(existing) == source:
            return name

    return None


# ============================================================
# UI
# ============================================================

def print_header():
    print()
    print("=" * 60)
    print(" DragonRise → Xbox 360 Mapper")
    print("=" * 60)
    print()


def print_control(
    index,
    total,
    control,
):
    print()
    print(
        f"[{index}/{total}]"
    )
    print()
    print(
        "─" * 60
    )
    print(
        f"  {control['display']}"
    )
    print(
        "─" * 60
    )
    print(
        control["instruction"]
    )
    print()
    print(
        "Press ENTER only if this control does not exist."
    )
    print()


def print_mapping(mapping):
    print()
    print(
        f"  ✓ {mapping['name']} "
        f"(code {mapping['code']}, "
        f"value {mapping['value']})"
    )


# ============================================================
# Main mapping loop
# ============================================================

def run_mapping(
    fd,
    output,
):
    mappings = {}

    total = len(CONTROLS)

    print()
    print("=" * 60)
    print(" MAPPING")
    print("=" * 60)
    print()
    print(
        "For every control:"
    )
    print(
        "  • perform the requested action"
    )
    print(
        "  • RELEASE it completely"
    )
    print(
        "  • the program automatically continues"
    )
    print()
    print(
        "Press ENTER only when a control does not exist."
    )
    print()

    input(
        "Press ENTER to begin mapping..."
    )

    # Give the terminal/input device a moment to settle.
    time.sleep(0.2)

    drain_events(fd)

    with RawTerminal():
        for index, control in enumerate(
            CONTROLS,
            1,
        ):
            print_control(
                index,
                total,
                control,
            )

            # Drain anything generated before this control.
            drain_events(fd)

            # Clear any stale ENTER characters.
            while enter_pressed():
                pass

            print(
                "  Listening..."
            )

            skipped = False

            # ------------------------------------------------
            # Wait for either:
            #
            #   controller event
            #
            # OR
            #
            #   ENTER
            # ------------------------------------------------

            # We cannot simply call map_control() because it
            # would block forever and would not notice ENTER.
            #
            # Instead, temporarily use a polling loop that
            # handles skip + controller events.
            #

            mapping = map_control_with_skip(
                fd,
                control,
            )

            if mapping is None:
                print()
                print(
                    "  - Skipped."
                )
                continue

            duplicate = find_duplicate(
                mappings,
                mapping,
            )

            if duplicate is not None:
                print()
                print(
                    "  ! WARNING"
                )
                print(
                    f"  This physical input is already "
                    f"mapped to: {duplicate}"
                )
                print()
                print(
                    "  This usually means the previous "
                    "mapping was incorrect."
                )
                print()

                # We deliberately don't silently accept it.
                # Ask user whether to retry.
                print(
                    "  Press ENTER to retry this control."
                )

                while True:
                    if enter_pressed():
                        break

                    time.sleep(0.01)

                # Retry same control.
                # We need to put it back into the loop.
                # Easiest is recursive retry here.
                retry_mapping = retry_control(
                    fd,
                    control,
                )

                if retry_mapping is None:
                    print(
                        "  - Skipped."
                    )
                    continue

                mapping = retry_mapping

            mappings[
                control["name"]
            ] = mapping

            print_mapping(
                mapping
            )

            # Give the device a tiny settling period.
            time.sleep(0.05)

            # Very important:
            # remove release/noise events before the next
            # control.
            drain_events(fd)

    write_config(
        output,
        mappings,
    )

    print()
    print("=" * 60)
    print(" MAPPING COMPLETE")
    print("=" * 60)
    print()

    print(
        f"Configuration written to:"
    )
    print(
        f"  {output}"
    )
    print()

    print(
        "Mappings:"
    )
    print()

    for control in CONTROLS:
        name = control["name"]

        if name in mappings:
            mapping = mappings[name]

            print(
                f"  {control['display']:<28} "
                f"{mapping['name']:<16} "
                f"code={mapping['code']:<3} "
                f"value={mapping['value']}"
            )
        else:
            print(
                f"  {control['display']:<28} "
                f"DISABLED"
            )

    print()


# ============================================================
# Skip-aware detection
# ============================================================

def wait_event_with_skip(fd):
    """
    Wait for either:

      - a controller event
      - ENTER

    Returns:

      ("event", event)
      ("skip", None)
    """
    while True:
        if enter_pressed():
            return "skip", None

        event = wait_for_event(
            fd,
            timeout=0.02,
        )

        if event is not None:
            return "event", event


def detect_axis_with_skip(fd):
    """
    Axis detector with ENTER support.
    """
    while True:
        kind, event = wait_event_with_skip(fd)

        if kind == "skip":
            return None

        event_type, code, value = event

        if event_type != EV_ABS:
            continue

        if code not in AXIS_CODES:
            continue

        if not axis_is_moved(value):
            continue

        initial_value = value

        # ----------------------------------------------------
        # IMPORTANT:
        #
        # The axis has been detected.
        #
        # Now WAIT FOR RELEASE.
        #
        # This prevents the release from becoming the next
        # mapping.
        # ----------------------------------------------------

        deadline = time.monotonic() + RELEASE_TIMEOUT

        while time.monotonic() < deadline:
            event = wait_for_event(
                fd,
                timeout=0.02,
            )

            if event is None:
                continue

            event_type2, code2, value2 = event

            if (
                event_type2 == EV_ABS
                and code2 == code
                and axis_is_center(value2)
            ):
                return {
                    "type": "abs",
                    "code": code,
                    "value": initial_value,
                    "name": event_name(
                        EV_ABS,
                        code,
                    ),
                }


def detect_button_with_skip(fd):
    """
    Button detector with ENTER support.
    """
    while True:
        kind, event = wait_event_with_skip(fd)

        if kind == "skip":
            return None

        event_type, code, value = event

        if event_type != EV_KEY:
            continue

        if value != 1:
            continue

        # Wait for physical release.
        deadline = (
            time.monotonic()
            + RELEASE_TIMEOUT
        )

        while time.monotonic() < deadline:
            event2 = wait_for_event(
                fd,
                timeout=0.02,
            )

            if event2 is None:
                continue

            type2, code2, value2 = event2

            if (
                type2 == EV_KEY
                and code2 == code
                and value2 == 0
            ):
                break

        return {
            "type": "key",
            "code": code,
            "value": 1,
            "name": event_name(
                EV_KEY,
                code,
            ),
        }


def detect_trigger_with_skip(fd):
    """
    Trigger detector.

    Supports both analog ABS triggers and digital KEY triggers.
    """
    while True:
        kind, event = wait_event_with_skip(fd)

        if kind == "skip":
            return None

        event_type, code, value = event

        # Analog trigger
        if (
            event_type == EV_ABS
            and code in AXIS_CODES
        ):
            if not axis_is_moved(value):
                continue

            initial_value = value

            deadline = (
                time.monotonic()
                + RELEASE_TIMEOUT
            )

            while time.monotonic() < deadline:
                event2 = wait_for_event(
                    fd,
                    timeout=0.02,
                )

                if event2 is None:
                    continue

                type2, code2, value2 = event2

                if (
                    type2 == EV_ABS
                    and code2 == code
                    and axis_is_center(value2)
                ):
                    return {
                        "type": "abs",
                        "code": code,
                        "value": initial_value,
                        "name": event_name(
                            EV_ABS,
                            code,
                        ),
                    }

        # Digital trigger
        elif (
            event_type == EV_KEY
            and value == 1
        ):
            deadline = (
                time.monotonic()
                + RELEASE_TIMEOUT
            )

            while time.monotonic() < deadline:
                event2 = wait_for_event(
                    fd,
                    timeout=0.02,
                )

                if event2 is None:
                    continue

                type2, code2, value2 = event2

                if (
                    type2 == EV_KEY
                    and code2 == code
                    and value2 == 0
                ):
                    break

            return {
                "type": "key",
                "code": code,
                "value": 1,
                "name": event_name(
                    EV_KEY,
                    code,
                ),
            }


def detect_dpad_with_skip(fd, control_name):
    if control_name == "dpad_left":
        wanted_code = 16
        wanted_value = -1

    elif control_name == "dpad_right":
        wanted_code = 16
        wanted_value = 1

    elif control_name == "dpad_up":
        wanted_code = 17
        wanted_value = -1

    elif control_name == "dpad_down":
        wanted_code = 17
        wanted_value = 1

    else:
        raise ValueError(control_name)

    while True:
        kind, event = wait_event_with_skip(fd)

        if kind == "skip":
            return None

        event_type, code, value = event

        if event_type != EV_ABS:
            continue

        if code != wanted_code:
            continue

        # Normalize unsigned -1.
        if value == 0xFFFFFFFF:
            value = -1

        if value != wanted_value:
            continue

        # Wait for D-pad release.
        deadline = (
            time.monotonic()
            + RELEASE_TIMEOUT
        )

        while time.monotonic() < deadline:
            event2 = wait_for_event(
                fd,
                timeout=0.02,
            )

            if event2 is None:
                continue

            type2, code2, value2 = event2

            if (
                type2 == EV_ABS
                and code2 == wanted_code
            ):
                if value2 == 0:
                    break

        return {
            "type": "abs",
            "code": wanted_code,
            "value": wanted_value,
            "name": event_name(
                EV_ABS,
                wanted_code,
            ),
        }


def map_control_with_skip(fd, control):
    kind = control["kind"]

    if kind == "axis":
        return detect_axis_with_skip(fd)

    if kind == "button":
        return detect_button_with_skip(fd)

    if kind == "trigger":
        return detect_trigger_with_skip(fd)

    if kind == "dpad":
        return detect_dpad_with_skip(
            fd,
            control["name"],
        )

    raise RuntimeError(
        f"Unknown control type: {kind}"
    )


def retry_control(fd, control):
    drain_events(fd)

    print()
    print(
        "  RETRY"
    )
    print(
        "  Perform the requested action again."
    )
    print()

    return map_control_with_skip(
        fd,
        control,
    )


# ============================================================
# Entry point
# ============================================================

def main():
    print_header()

    device = choose_controller()

    output = "mohamed.conf"

    try:
        fd = os.open(
            device,
            os.O_RDONLY | os.O_NONBLOCK,
        )

    except PermissionError:
        print()
        print(
            "ERROR: Permission denied."
        )
        print()
        print(
            "Run the mapper with:"
        )
        print()
        print(
            f"  sudo {sys.argv[0]}"
        )
        print()
        sys.exit(1)

    except OSError as e:
        print()
        print(
            f"ERROR: Could not open controller:"
        )
        print(
            f"  {e}"
        )
        print()
        sys.exit(1)

    try:
        # Let the device settle.
        time.sleep(
            STARTUP_DRAIN_TIME
        )

        drain_events(fd)

        run_mapping(
            fd,
            output,
        )

    except KeyboardInterrupt:
        print()
        print()
        print(
            "Mapping cancelled."
        )
        print()

    finally:
        os.close(fd)


if __name__ == "__main__":
    main()