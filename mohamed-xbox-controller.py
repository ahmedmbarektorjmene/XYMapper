#!/usr/bin/env python3

import glob
import os
import select
import struct
import subprocess
import sys
import termios
import tty
import time


# ============================================================
# Mohamed Controller
#
# Physical controller
#        ↓
#      evdev
#        ↓
#      xboxdrv
#        ↓
# Xbox-compatible virtual controller
#
# Usage:
#
#   ./mohamed.py
#
# Or:
#
#   sudo ./mohamed.py
#
# The generated mapping is:
#
#   mohamed.conf
# ============================================================


CONFIG_FILE = "mohamed.conf"


# ============================================================
# Linux input definitions
# ============================================================

EVENT_STRUCT = struct.Struct("llHHI")

EV_SYN = 0
EV_KEY = 1
EV_ABS = 3


# ============================================================
# Linux EV_KEY names
#
# These are important because xboxdrv expects the symbolic
# evdev names, NOT KEY_304 / KEY_305 / etc.
# ============================================================

KEY_NAMES = {
    # --------------------------------------------------------
    # Standard keyboard keys
    # --------------------------------------------------------

    1: "KEY_ESC",
    2: "KEY_1",
    3: "KEY_2",
    4: "KEY_3",
    5: "KEY_4",
    6: "KEY_5",
    7: "KEY_6",
    8: "KEY_7",
    9: "KEY_8",
    10: "KEY_9",
    11: "KEY_0",

    # --------------------------------------------------------
    # Gamepad buttons
    #
    # Linux input-event-codes.h
    # --------------------------------------------------------

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

    # --------------------------------------------------------
    # Common gamepad / joystick codes
    # --------------------------------------------------------

    304: "BTN_SOUTH",
    305: "BTN_EAST",
    306: "BTN_C",
    307: "BTN_NORTH",
    308: "BTN_WEST",
    309: "BTN_Z",
    310: "BTN_TL",
    311: "BTN_TR",
    312: "BTN_TL2",
    313: "BTN_TR2",
    314: "BTN_SELECT",
    315: "BTN_START",
    316: "BTN_MODE",
    317: "BTN_THUMBL",
    318: "BTN_THUMBR",
    319: "BTN_TOOL_QUINTTAP",
}


# ============================================================
# ABS names
# ============================================================

ABS_NAMES = {
    0: "ABS_X",
    1: "ABS_Y",
    2: "ABS_Z",
    3: "ABS_RX",
    4: "ABS_RY",
    5: "ABS_RZ",

    16: "ABS_HAT0X",
    17: "ABS_HAT0Y",

    18: "ABS_HAT1X",
    19: "ABS_HAT1Y",

    20: "ABS_HAT2X",
    21: "ABS_HAT2Y",

    22: "ABS_HAT3X",
    23: "ABS_HAT3Y",

    24: "ABS_HAT4X",
    25: "ABS_HAT4Y",
}


AXIS_CODES = {
    0,
    1,
    2,
    3,
    4,
    5,
}


DPAD_CODES = {
    16,
    17,
}


# ============================================================
# Center detection
# ============================================================

CENTER_MIN = 100
CENTER_MAX = 155


# ============================================================
# Timing
# ============================================================

RELEASE_TIMEOUT = 2.0

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
        "instruction": "Press GUIDE / HOME if available, then RELEASE it.",
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

        self.old = termios.tcgetattr(
            self.fd
        )

        tty.setcbreak(
            self.fd
        )

        return self

    def __exit__(
        self,
        exc_type,
        exc_value,
        traceback,
    ):
        termios.tcsetattr(
            self.fd,
            termios.TCSADRAIN,
            self.old,
        )


def enter_pressed():
    """
    Non-blocking ENTER detection.
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

    return (
        b"\n" in data
        or b"\r" in data
    )


# ============================================================
# evdev helpers
# ============================================================

def event_name(
    event_type,
    code,
):
    if event_type == EV_KEY:

        return KEY_NAMES.get(
            code,
            f"KEY_{code}",
        )

    if event_type == EV_ABS:

        return ABS_NAMES.get(
            code,
            f"ABS_{code}",
        )

    return (
        f"TYPE_{event_type}_CODE_{code}"
    )


def normalize_key_name(name):
    """
    Convert old generated names such as:

        KEY_304
        KEY_305
        KEY_308

    into names understood by xboxdrv:

        BTN_SOUTH
        BTN_EAST
        BTN_WEST

    This also makes old mohamed.conf files compatible.
    """

    if not name:
        return name

    if name.startswith("KEY_"):

        number = name[4:]

        try:
            code = int(number)

            return KEY_NAMES.get(
                code,
                name,
            )

        except ValueError:
            pass

    return name


def read_event(fd):

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

    _, _, event_type, code, value = (
        EVENT_STRUCT.unpack(data)
    )

    return (
        event_type,
        code,
        value,
    )


def drain_events(fd):

    while True:

        try:

            ready, _, _ = select.select(
                [fd],
                [],
                [],
                0,
            )

        except (
            OSError,
            ValueError,
        ):

            return

        if not ready:
            break

        event = read_event(fd)

        if event is None:
            break


def axis_is_center(value):

    return (
        CENTER_MIN
        <= value
        <= CENTER_MAX
    )


def axis_is_moved(value):

    return not axis_is_center(
        value
    )


# ============================================================
# Event waiting
# ============================================================

def wait_for_event(
    fd,
    timeout=0.05,
):

    ready, _, _ = select.select(
        [fd],
        [],
        [],
        timeout,
    )

    if not ready:
        return None

    return read_event(fd)


def wait_event_with_skip(fd):

    while True:

        if enter_pressed():

            return (
                "skip",
                None,
            )

        event = wait_for_event(
            fd,
            timeout=0.02,
        )

        if event is not None:

            return (
                "event",
                event,
            )


# ============================================================
# Button release
# ============================================================

def wait_button_release(
    fd,
    code,
):

    deadline = (
        time.monotonic()
        + RELEASE_TIMEOUT
    )

    while (
        time.monotonic()
        < deadline
    ):

        event = wait_for_event(
            fd,
            timeout=0.02,
        )

        if event is None:
            continue

        event_type, event_code, value = (
            event
        )

        if event_type != EV_KEY:
            continue

        if event_code != code:
            continue

        if value == 0:
            return True

    return False


# ============================================================
# Axis release
# ============================================================

def wait_axis_release(
    fd,
    code,
):

    deadline = (
        time.monotonic()
        + RELEASE_TIMEOUT
    )

    while (
        time.monotonic()
        < deadline
    ):

        event = wait_for_event(
            fd,
            timeout=0.02,
        )

        if event is None:
            continue

        event_type, event_code, value = (
            event
        )

        if event_type != EV_ABS:
            continue

        if event_code != code:
            continue

        if axis_is_center(value):

            return True

    return False


# ============================================================
# D-pad release
# ============================================================

def wait_dpad_release(
    fd,
    code,
):

    deadline = (
        time.monotonic()
        + RELEASE_TIMEOUT
    )

    while (
        time.monotonic()
        < deadline
    ):

        event = wait_for_event(
            fd,
            timeout=0.02,
        )

        if event is None:
            continue

        event_type, event_code, value = (
            event
        )

        if event_type != EV_ABS:
            continue

        if event_code != code:
            continue

        if value == 0:

            return True

    return False


# ============================================================
# Detect axis
# ============================================================

def detect_axis_with_skip(fd):

    while True:

        kind, event = (
            wait_event_with_skip(fd)
        )

        if kind == "skip":
            return None

        event_type, code, value = (
            event
        )

        if event_type != EV_ABS:
            continue

        if code not in AXIS_CODES:
            continue

        if not axis_is_moved(value):
            continue

        initial_value = value

        wait_axis_release(
            fd,
            code,
        )

        return {
            "type": "abs",
            "code": code,
            "value": initial_value,
            "name": event_name(
                EV_ABS,
                code,
            ),
        }


# ============================================================
# Detect button
# ============================================================

def detect_button_with_skip(fd):

    while True:

        kind, event = (
            wait_event_with_skip(fd)
        )

        if kind == "skip":
            return None

        event_type, code, value = (
            event
        )

        if event_type != EV_KEY:
            continue

        if value != 1:
            continue

        wait_button_release(
            fd,
            code,
        )

        return {
            "type": "key",
            "code": code,
            "value": 1,
            "name": event_name(
                EV_KEY,
                code,
            ),
        }


# ============================================================
# Detect trigger
# ============================================================

def detect_trigger_with_skip(fd):

    while True:

        kind, event = (
            wait_event_with_skip(fd)
        )

        if kind == "skip":
            return None

        event_type, code, value = (
            event
        )

        # ----------------------------------------------------
        # Analog trigger
        # ----------------------------------------------------

        if (
            event_type == EV_ABS
            and code in AXIS_CODES
        ):

            if not axis_is_moved(value):
                continue

            initial_value = value

            wait_axis_release(
                fd,
                code,
            )

            return {
                "type": "abs",
                "code": code,
                "value": initial_value,
                "name": event_name(
                    EV_ABS,
                    code,
                ),
            }

        # ----------------------------------------------------
        # Digital trigger
        # ----------------------------------------------------

        if (
            event_type == EV_KEY
            and value == 1
        ):

            wait_button_release(
                fd,
                code,
            )

            return {
                "type": "key",
                "code": code,
                "value": 1,
                "name": event_name(
                    EV_KEY,
                    code,
                ),
            }


# ============================================================
# Detect D-pad
# ============================================================

def detect_dpad_with_skip(
    fd,
    control_name,
):

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

        raise ValueError(
            control_name
        )

    while True:

        kind, event = (
            wait_event_with_skip(fd)
        )

        if kind == "skip":
            return None

        event_type, code, value = (
            event
        )

        if event_type != EV_ABS:
            continue

        if code != wanted_code:
            continue

        # Normalize unsigned -1.
        if value == 0xFFFFFFFF:
            value = -1

        if value != wanted_value:
            continue

        wait_dpad_release(
            fd,
            wanted_code,
        )

        return {
            "type": "abs",
            "code": wanted_code,
            "value": wanted_value,
            "name": event_name(
                EV_ABS,
                wanted_code,
            ),
        }


# ============================================================
# Map control
# ============================================================

def map_control_with_skip(
    fd,
    control,
):

    kind = control["kind"]

    if kind == "axis":

        return detect_axis_with_skip(
            fd
        )

    if kind == "button":

        return detect_button_with_skip(
            fd
        )

    if kind == "trigger":

        return detect_trigger_with_skip(
            fd
        )

    if kind == "dpad":

        return detect_dpad_with_skip(
            fd,
            control["name"],
        )

    raise RuntimeError(
        f"Unknown control type: {kind}"
    )


# ============================================================
# Controller discovery
# ============================================================

def find_controllers():

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

            real_path = os.path.realpath(
                path
            )

            if not real_path.startswith(
                "/dev/input/event"
            ):
                continue

            controllers.append(
                {
                    "name": os.path.basename(
                        path
                    ),
                    "path": path,
                    "real_path": real_path,
                }
            )

        except OSError:
            continue

    return controllers


# ============================================================
# Controller selection
# ============================================================

def choose_controller():

    controllers = find_controllers()

    if not controllers:

        print()
        print(
            "ERROR: No joystick/controller "
            "devices found."
        )
        print()
        print(
            "Check:"
        )
        print(
            "  ls -l /dev/input/by-id/"
        )
        print()

        sys.exit(1)

    print()
    print(
        "=" * 60
    )
    print(
        " CONTROLLER SELECTION"
    )
    print(
        "=" * 60
    )
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
            f"      Device: "
            f"{controller['path']}"
        )

        print(
            f"      Event:  "
            f"{controller['real_path']}"
        )

        print()

    while True:

        try:

            answer = input(
                f"Select controller "
                f"[1-{len(controllers)}]: "
            ).strip()

            index = int(answer)

            if (
                1
                <= index
                <= len(controllers)
            ):

                selected = (
                    controllers[index - 1]
                )

                print()
                print(
                    "Selected controller:"
                )

                print(
                    f"  {selected['name']}"
                )

                print(
                    f"  {selected['path']}"
                )

                print()

                return selected["path"]

        except (
            ValueError,
            EOFError,
        ):

            pass

        print()
        print(
            f"Please enter a number "
            f"from 1 to "
            f"{len(controllers)}."
        )


# ============================================================
# Configuration loading
# ============================================================

def trim(value):

    return value.strip()


def load_config(path):

    mappings = {}

    if not os.path.isfile(path):

        print()
        print(
            "ERROR: Mapping configuration "
            "not found:"
        )
        print(
            f"  {path}"
        )
        print()

        sys.exit(1)

    with open(
        path,
        "r",
        encoding="utf-8",
    ) as f:

        for raw_line in f:

            line = raw_line.strip()

            if not line:
                continue

            if line.startswith("#"):
                continue

            if "=" not in line:
                continue

            key, value = (
                line.split(
                    "=",
                    1,
                )
            )

            key = trim(key)
            value = trim(value)

            if not key:
                continue

            if value == "disabled":

                mappings[key] = {
                    "type": "disabled",
                    "code": "",
                    "value": "",
                    "name": "",
                }

                continue

            parts = [
                trim(x)
                for x in value.split(",")
            ]

            if len(parts) < 4:
                continue

            mapping_type = parts[0]
            code = parts[1]
            axis_value = parts[2]
            name = parts[3]

            # ------------------------------------------------
            # IMPORTANT:
            #
            # Old configs may contain:
            #
            #   KEY_304
            #
            # Normalize it now.
            # ------------------------------------------------

            if mapping_type == "key":

                name = normalize_key_name(
                    name
                )

            mappings[key] = {
                "type": mapping_type,
                "code": code,
                "value": axis_value,
                "name": name,
            }

    return mappings


# ============================================================
# Mapping aliases
# ============================================================

def alias_mapping(
    mappings,
    canonical,
    *aliases,
):

    if canonical in mappings:
        return

    for alias in aliases:

        if alias in mappings:

            mappings[canonical] = (
                mappings[alias].copy()
            )

            return


# ============================================================
# Mapping validation
# ============================================================

def require_mapping(
    mappings,
    name,
):

    if name not in mappings:

        print()
        print(
            f"ERROR: Required mapping "
            f"'{name}' is missing."
        )
        print()

        sys.exit(1)

    if mappings[name]["type"] == "disabled":

        print()
        print(
            f"ERROR: Required mapping "
            f"'{name}' is disabled."
        )
        print()

        sys.exit(1)


def validate_key_mapping(
    mappings,
    name,
):

    require_mapping(
        mappings,
        name,
    )

    mapping = mappings[name]

    if mapping["type"] != "key":

        print()
        print(
            f"ERROR: '{name}' is not "
            f"a key mapping."
        )

        print(
            f"Detected type: "
            f"{mapping['type']}"
        )

        print()

        sys.exit(1)

    return normalize_key_name(
        mapping["name"]
    )


def validate_abs_mapping(
    mappings,
    name,
):

    require_mapping(
        mappings,
        name,
    )

    mapping = mappings[name]

    if mapping["type"] != "abs":

        print()
        print(
            f"ERROR: '{name}' is not "
            f"an ABS mapping."
        )

        print(
            f"Detected type: "
            f"{mapping['type']}"
        )

        print()

        sys.exit(1)

    return mapping["name"]


# ============================================================
# Build xboxdrv mapping
# ============================================================

def build_xboxdrv_mapping(
    mappings
):

    # --------------------------------------------------------
    # Aliases
    # --------------------------------------------------------

    alias_mapping(
        mappings,
        "left_trigger",
        "l2",
        "L2",
    )

    alias_mapping(
        mappings,
        "right_trigger",
        "r2",
        "R2",
    )

    alias_mapping(
        mappings,
        "lb",
        "l1",
        "L1",
    )

    alias_mapping(
        mappings,
        "rb",
        "r1",
        "R1",
    )

    alias_mapping(
        mappings,
        "left_stick_click",
        "l3",
        "L3",
    )

    alias_mapping(
        mappings,
        "right_stick_click",
        "r3",
        "R3",
    )

    # --------------------------------------------------------
    # ABS mappings
    # --------------------------------------------------------

    abs_map = []

    abs_map.append(
        "{}=x1".format(
            validate_abs_mapping(
                mappings,
                "left_stick_x",
            )
        )
    )

    abs_map.append(
        "{}=y1".format(
            validate_abs_mapping(
                mappings,
                "left_stick_y",
            )
        )
    )

    abs_map.append(
        "{}=x2".format(
            validate_abs_mapping(
                mappings,
                "right_stick_x",
            )
        )
    )

    abs_map.append(
        "{}=y2".format(
            validate_abs_mapping(
                mappings,
                "right_stick_y",
            )
        )
    )

    # --------------------------------------------------------
    # L2
    # --------------------------------------------------------

    abs_map.append(
        "{}=lt".format(
            validate_abs_mapping(
                mappings,
                "left_trigger",
            )
        )
    )

    # --------------------------------------------------------
    # D-pad
    #
    # Both directions intentionally use the same physical axis.
    # xboxdrv interprets dpad_x/dpad_y.
    # --------------------------------------------------------

    dpad_x = validate_abs_mapping(
        mappings,
        "dpad_left",
    )

    dpad_y = validate_abs_mapping(
        mappings,
        "dpad_up",
    )

    abs_map.append(
        f"{dpad_x}=dpad_x"
    )

    abs_map.append(
        f"{dpad_y}=dpad_y"
    )

    # --------------------------------------------------------
    # KEY mappings
    # --------------------------------------------------------

    key_map = []

    key_map.append(
        "{}=a".format(
            validate_key_mapping(
                mappings,
                "a",
            )
        )
    )

    key_map.append(
        "{}=b".format(
            validate_key_mapping(
                mappings,
                "b",
            )
        )
    )

    key_map.append(
        "{}=x".format(
            validate_key_mapping(
                mappings,
                "x",
            )
        )
    )

    key_map.append(
        "{}=y".format(
            validate_key_mapping(
                mappings,
                "y",
            )
        )
    )

    key_map.append(
        "{}=lb".format(
            validate_key_mapping(
                mappings,
                "lb",
            )
        )
    )

    key_map.append(
        "{}=rb".format(
            validate_key_mapping(
                mappings,
                "rb",
            )
        )
    )

    key_map.append(
        "{}=tl".format(
            validate_key_mapping(
                mappings,
                "left_stick_click",
            )
        )
    )

    key_map.append(
        "{}=tr".format(
            validate_key_mapping(
                mappings,
                "right_stick_click",
            )
        )
    )

    key_map.append(
        "{}=back".format(
            validate_key_mapping(
                mappings,
                "back",
            )
        )
    )

    key_map.append(
        "{}=start".format(
            validate_key_mapping(
                mappings,
                "start",
            )
        )
    )

    # --------------------------------------------------------
    # R2
    #
    # Your DS4 mapping currently reports:
    #
    #   KEY_313
    #
    # which is BTN_TR2.
    #
    # xboxdrv accepts BTN_TR2.
    # --------------------------------------------------------

    right_trigger = mappings.get(
        "right_trigger"
    )

    if right_trigger is not None:

        if right_trigger["type"] == "key":

            key_map.append(
                "{}=rt".format(
                    validate_key_mapping(
                        mappings,
                        "right_trigger",
                    )
                )
            )

        elif right_trigger["type"] == "abs":

            abs_map.append(
                "{}=rt".format(
                    validate_abs_mapping(
                        mappings,
                        "right_trigger",
                    )
                )
            )

        else:

            print()
            print(
                "ERROR: Invalid R2 mapping."
            )

            sys.exit(1)

    # --------------------------------------------------------
    # Guide
    # --------------------------------------------------------

    if "guide" in mappings:

        guide = mappings["guide"]

        if guide["type"] == "key":

            guide_name = (
                normalize_key_name(
                    guide["name"]
                )
            )

            key_map.append(
                f"{guide_name}=guide"
            )

    return (
        abs_map,
        key_map,
    )


# ============================================================
# Display mapping
# ============================================================

def display_mapping(
    mappings
):

    print()
    print(
        "=" * 60
    )
    print(
        " FINAL CONTROLLER MAPPING"
    )
    print(
        "=" * 60
    )
    print()

    print("Analog:")

    print(
        f"  {'LEFT STICK X':<24} → "
        f"{mappings['left_stick_x']['name']}"
    )

    print(
        f"  {'LEFT STICK Y':<24} → "
        f"{mappings['left_stick_y']['name']}"
    )

    print(
        f"  {'RIGHT STICK X':<24} → "
        f"{mappings['right_stick_x']['name']}"
    )

    print(
        f"  {'RIGHT STICK Y':<24} → "
        f"{mappings['right_stick_y']['name']}"
    )

    print(
        f"  {'L2 — ANALOG':<24} → "
        f"{mappings['left_trigger']['name']}"
    )

    print()
    print("Buttons:")

    for display_name, key in [
        ("A", "a"),
        ("B", "b"),
        ("X", "x"),
        ("Y", "y"),
        ("L1", "lb"),
        ("R1", "rb"),
        ("R2", "right_trigger"),
        ("L3", "left_stick_click"),
        ("R3", "right_stick_click"),
        ("BACK", "back"),
        ("START", "start"),
    ]:

        if key not in mappings:
            continue

        mapping = mappings[key]

        print(
            f"  {display_name:<24} → "
            f"{normalize_key_name(mapping['name'])}"
        )

    print()
    print("D-pad:")

    print(
        f"  {'LEFT':<24} → "
        f"{mappings['dpad_left']['name']}"
    )

    print(
        f"  {'RIGHT':<24} → "
        f"{mappings['dpad_right']['name']}"
    )

    print(
        f"  {'UP':<24} → "
        f"{mappings['dpad_up']['name']}"
    )

    print(
        f"  {'DOWN':<24} → "
        f"{mappings['dpad_down']['name']}"
    )

    print()


# ============================================================
# Execute xboxdrv
# ============================================================

def start_xboxdrv(
    device,
    mappings,
):

    abs_map, key_map = (
        build_xboxdrv_mapping(
            mappings
        )
    )

    abs_string = ",".join(
        abs_map
    )

    key_string = ",".join(
        key_map
    )

    args = [
        "xboxdrv",

        "--evdev",
        device,

        "--mimic-xpad",

        "--evdev-absmap",
        abs_string,

        "--evdev-keymap",
        key_string,
    ]

    # --------------------------------------------------------
    # Display exact command
    # --------------------------------------------------------

    print()
    print(
        "=" * 60
    )
    print(
        " STARTING XBOX-COMPATIBLE EMULATION"
    )
    print(
        "=" * 60
    )
    print()

    print("Physical:")
    print(
        f"  {device}"
    )

    print()

    print("Virtual:")
    print(
        "  Xbox-compatible XInput controller"
    )

    print()

    print("Configuration:")
    print(
        f"  {CONFIG_FILE}"
    )

    print()

    print(
        "Press Ctrl+C to stop."
    )

    print()

    print("Executing:")
    print()

    print(
        "  "
        + " ".join(
            args
        )
    )

    print()

    # --------------------------------------------------------
    # Execute
    # --------------------------------------------------------

    try:

        return subprocess.call(
            args
        )

    except FileNotFoundError:

        print()
        print(
            "ERROR: xboxdrv was not found."
        )

        print()
        print(
            "Install xboxdrv first."
        )

        print()

        return 1

    except KeyboardInterrupt:

        print()
        print()
        print(
            "Stopping xboxdrv..."
        )

        return 0


# ============================================================
# Main
# ============================================================

def main():

    print()
    print(
        "=" * 60
    )
    print(
        " Controller → Xbox-compatible Mapper"
    )
    print(
        "=" * 60
    )
    print()

    # --------------------------------------------------------
    # Configuration
    # --------------------------------------------------------

    print(
        "==> Loading mapping:"
    )

    print(
        f"  {CONFIG_FILE}"
    )

    print()

    mappings = load_config(
        CONFIG_FILE
    )

    # --------------------------------------------------------
    # Controller selection
    # --------------------------------------------------------

    device = choose_controller()

    # --------------------------------------------------------
    # Normalize old config names
    # --------------------------------------------------------

    for mapping in mappings.values():

        if mapping["type"] == "key":

            mapping["name"] = (
                normalize_key_name(
                    mapping["name"]
                )
            )

    # --------------------------------------------------------
    # Translate / validate
    # --------------------------------------------------------

    print(
        "==> Translating controller mapping..."
    )

    # Force aliases before display.
    alias_mapping(
        mappings,
        "left_trigger",
        "l2",
        "L2",
    )

    alias_mapping(
        mappings,
        "right_trigger",
        "r2",
        "R2",
    )

    alias_mapping(
        mappings,
        "lb",
        "l1",
        "L1",
    )

    alias_mapping(
        mappings,
        "rb",
        "r1",
        "R1",
    )

    alias_mapping(
        mappings,
        "left_stick_click",
        "l3",
        "L3",
    )

    alias_mapping(
        mappings,
        "right_stick_click",
        "r3",
        "R3",
    )

    # --------------------------------------------------------
    # Validate everything before starting xboxdrv
    # --------------------------------------------------------

    required = [
        "left_stick_x",
        "left_stick_y",
        "right_stick_x",
        "right_stick_y",

        "left_trigger",

        "a",
        "b",
        "x",
        "y",

        "lb",
        "rb",

        "left_stick_click",
        "right_stick_click",

        "back",
        "start",

        "dpad_left",
        "dpad_right",
        "dpad_up",
        "dpad_down",
    ]

    for name in required:

        require_mapping(
            mappings,
            name,
        )

    display_mapping(
        mappings
    )

    # --------------------------------------------------------
    # Start
    # --------------------------------------------------------

    sys.exit(
        start_xboxdrv(
            device,
            mappings,
        )
    )


# ============================================================
# Entry point
# ============================================================

if __name__ == "__main__":
    main()