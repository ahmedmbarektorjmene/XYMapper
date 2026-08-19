#!/usr/bin/env bash
set -euo pipefail

VID="0079"
PID="0006"

CONFIG_FILE="${1:-controller-map.conf}"

echo "==> Checking for DragonRise PC TWIN SHOCK ($VID:$PID)..."

if ! lsusb | grep -qiE "ID[[:space:]]+$VID:$PID([[:space:]]|$)"; then
    echo "ERROR: DragonRise controller $VID:$PID not found."
    echo
    echo "Detected USB devices:"
    lsusb
    exit 1
fi

echo "==> DragonRise controller found."

# ============================================================
# CONFIGURATION
# ============================================================

if [[ ! -f "$CONFIG_FILE" ]]; then
    echo "ERROR: Mapping configuration not found:"
    echo "  $CONFIG_FILE"
    echo
    echo "Run:"
    echo "  sudo ./map_controller.py"
    exit 1
fi

echo "==> Loading mapping:"
echo "  $CONFIG_FILE"

declare -A MAP_TYPE
declare -A MAP_CODE
declare -A MAP_VALUE
declare -A MAP_NAME

trim() {
    local s="$1"

    s="${s#"${s%%[![:space:]]*}"}"
    s="${s%"${s##*[![:space:]]}"}"

    printf '%s' "$s"
}

while IFS= read -r line || [[ -n "$line" ]]; do

    line="${line//$'\r'/}"

    [[ -z "$(trim "$line")" ]] && continue

    [[ "$line" =~ ^[[:space:]]*# ]] && continue

    if [[ "$line" != *=* ]]; then
        echo "WARNING: ignoring malformed line:"
        echo "  $line"
        continue
    fi

    key="${line%%=*}"
    value="${line#*=}"

    key="$(trim "$key")"
    value="$(trim "$value")"

    IFS=',' read -r type code axis_value name <<< "$value"

    type="$(trim "${type:-}")"
    code="$(trim "${code:-}")"
    axis_value="$(trim "${axis_value:-}")"
    name="$(trim "${name:-}")"

    if [[ -z "$key" || -z "$type" ]]; then
        echo "WARNING: ignoring malformed mapping:"
        echo "  $line"
        continue
    fi

    MAP_TYPE["$key"]="$type"
    MAP_CODE["$key"]="${code:-}"
    MAP_VALUE["$key"]="${axis_value:-}"
    MAP_NAME["$key"]="${name:-}"

done < "$CONFIG_FILE"

# ============================================================
# Mapping aliases
# ============================================================

alias_mapping() {
    local canonical="$1"
    shift

    if [[ -n "${MAP_TYPE[$canonical]+x}" ]]; then
        return 0
    fi

    local alias

    for alias in "$@"; do
        if [[ -n "${MAP_TYPE[$alias]+x}" ]]; then
            MAP_TYPE["$canonical"]="${MAP_TYPE[$alias]}"
            MAP_CODE["$canonical"]="${MAP_CODE[$alias]}"
            MAP_VALUE["$canonical"]="${MAP_VALUE[$alias]}"
            MAP_NAME["$canonical"]="${MAP_NAME[$alias]}"
            return 0
        fi
    done
}

alias_mapping "left_trigger"  "l2" "L2"
alias_mapping "right_trigger" "r2" "R2"

alias_mapping "lb" "l1" "L1"
alias_mapping "rb" "r1" "R1"

alias_mapping "left_stick_click"  "l3" "L3"
alias_mapping "right_stick_click" "r3" "R3"

# ============================================================
# Mapping helpers
# ============================================================

require_mapping() {
    local name="$1"

    if [[ -z "${MAP_TYPE[$name]+x}" ]]; then
        echo
        echo "ERROR: Required mapping '$name' is missing."
        echo
        echo "Mappings actually loaded from $CONFIG_FILE:"
        for key in "${!MAP_TYPE[@]}"; do
            printf '  %-24s %s\n' "$key" "${MAP_NAME[$key]:-}"
        done
        echo
        exit 1
    fi

    if [[ "${MAP_TYPE[$name]}" == "disabled" ]]; then
        echo
        echo "ERROR: Required mapping '$name' is disabled."
        exit 1
    fi
}

add_key_mapping() {
    local source="$1"
    local target="$2"

    require_mapping "$source"

    if [[ "${MAP_TYPE[$source]}" != "key" ]]; then
        echo "ERROR: '$source' is not a key mapping."
        echo "       Detected type: ${MAP_TYPE[$source]}"
        exit 1
    fi

    local name="${MAP_NAME[$source]}"

    if [[ -z "$name" ]]; then
        echo "ERROR: '$source' has no evdev event name."
        exit 1
    fi

    EVDEV_KEYMAP+=("${name}=${target}")
}

add_abs_mapping() {
    local source="$1"
    local target="$2"

    require_mapping "$source"

    if [[ "${MAP_TYPE[$source]}" != "abs" ]]; then
        echo "ERROR: '$source' is not an absolute-axis mapping."
        echo "       Detected type: ${MAP_TYPE[$source]}"
        exit 1
    fi

    local name="${MAP_NAME[$source]}"

    if [[ -z "$name" ]]; then
        echo "ERROR: '$source' has no evdev event name."
        exit 1
    fi

    EVDEV_ABSMAP+=("${name}=${target}")
}

# ============================================================
# Find DragonRise event device
# ============================================================

EVENT_DEVICE=""

for event in /dev/input/event*; do
    [[ -e "$event" ]] || continue

    PROPERTIES="$(
        udevadm info \
            --query=property \
            --name="$event" \
            2>/dev/null || true
    )"

    if grep -q "^ID_VENDOR_ID=$VID$" <<< "$PROPERTIES" &&
       grep -q "^ID_MODEL_ID=$PID$" <<< "$PROPERTIES"; then

        EVENT_DEVICE="$event"
        break
    fi
done

if [[ -z "$EVENT_DEVICE" ]]; then
    echo "ERROR: Could not find the DragonRise event device."
    exit 1
fi

echo "==> Physical controller: $EVENT_DEVICE"

# ============================================================
# uinput
# ============================================================

if [[ ! -e /dev/uinput ]]; then
    echo "==> Loading uinput kernel module..."
    modprobe uinput
fi

if [[ ! -e /dev/uinput ]]; then
    echo "ERROR: /dev/uinput still does not exist."
    exit 1
fi

# ============================================================
# Build Xbox mapping
# ============================================================

EVDEV_ABSMAP=()
EVDEV_KEYMAP=()

echo
echo "==> Translating controller mapping..."

# ============================================================
# Analog sticks
# ============================================================

add_abs_mapping "left_stick_x"  "x1"
add_abs_mapping "left_stick_y"  "y1"

add_abs_mapping "right_stick_x" "x2"
add_abs_mapping "right_stick_y" "y2"

# ============================================================
# Face buttons
# ============================================================

add_key_mapping "a" "a"
add_key_mapping "b" "b"
add_key_mapping "x" "x"
add_key_mapping "y" "y"

# ============================================================
# L1 / R1
# ============================================================

add_key_mapping "lb" "lb"
add_key_mapping "rb" "rb"

# ============================================================
# L3 / R3
# ============================================================

add_key_mapping "left_stick_click"  "tl"
add_key_mapping "right_stick_click" "tr"

# ============================================================
# Back / Start
# ============================================================

add_key_mapping "back"  "back"
add_key_mapping "start" "start"

# ============================================================
# L2 / R2
# ============================================================

add_key_mapping "left_trigger"  "lt"
add_key_mapping "right_trigger" "rt"

# ============================================================
# D-pad
# ============================================================

require_mapping "dpad_left"
require_mapping "dpad_right"
require_mapping "dpad_up"
require_mapping "dpad_down"

if [[ "${MAP_TYPE[dpad_left]}" != "abs" ]] ||
   [[ "${MAP_TYPE[dpad_right]}" != "abs" ]] ||
   [[ "${MAP_TYPE[dpad_up]}" != "abs" ]] ||
   [[ "${MAP_TYPE[dpad_down]}" != "abs" ]]; then

    echo "ERROR: D-pad mappings must be ABS mappings."
    exit 1
fi

D_PAD_X="${MAP_NAME[dpad_left]}"
D_PAD_Y="${MAP_NAME[dpad_up]}"

if [[ -z "$D_PAD_X" || -z "$D_PAD_Y" ]]; then
    echo "ERROR: D-pad mapping has no evdev axis name."
    exit 1
fi

EVDEV_ABSMAP+=("${D_PAD_X}=dpad_x")
EVDEV_ABSMAP+=("${D_PAD_Y}=dpad_y")

# ============================================================
# Display mapping
# ============================================================

echo
echo "============================================================"
echo " FINAL XBOX MAPPING"
echo "============================================================"
echo

echo "Analog:"
printf '  %-24s → %s\n' \
    "LEFT STICK X" \
    "${MAP_NAME[left_stick_x]}"

printf '  %-24s → %s\n' \
    "LEFT STICK Y" \
    "${MAP_NAME[left_stick_y]}"

printf '  %-24s → %s\n' \
    "RIGHT STICK X" \
    "${MAP_NAME[right_stick_x]}"

printf '  %-24s → %s\n' \
    "RIGHT STICK Y" \
    "${MAP_NAME[right_stick_y]}"

echo
echo "Buttons:"
printf '  %-24s → %s\n' "A" "${MAP_NAME[a]}"
printf '  %-24s → %s\n' "B" "${MAP_NAME[b]}"
printf '  %-24s → %s\n' "X" "${MAP_NAME[x]}"
printf '  %-24s → %s\n' "Y" "${MAP_NAME[y]}"

echo
printf '  %-24s → %s\n' "L1" "${MAP_NAME[lb]}"
printf '  %-24s → %s\n' "L2" "${MAP_NAME[left_trigger]}"
printf '  %-24s → %s\n' "L3" "${MAP_NAME[left_stick_click]}"

printf '  %-24s → %s\n' "R1" "${MAP_NAME[rb]}"
printf '  %-24s → %s\n' "R2" "${MAP_NAME[right_trigger]}"
printf '  %-24s → %s\n' "R3" "${MAP_NAME[right_stick_click]}"

echo
printf '  %-24s → %s\n' "BACK" "${MAP_NAME[back]}"
printf '  %-24s → %s\n' "START" "${MAP_NAME[start]}"

echo
echo "D-pad:"
printf '  %-24s → %s\n' "LEFT"  "${MAP_NAME[dpad_left]}"
printf '  %-24s → %s\n' "RIGHT" "${MAP_NAME[dpad_right]}"
printf '  %-24s → %s\n' "UP"    "${MAP_NAME[dpad_up]}"
printf '  %-24s → %s\n' "DOWN"  "${MAP_NAME[dpad_down]}"

echo
echo "Axis correction:"
echo "  LEFT STICK Y  → inverted"
echo "  RIGHT STICK Y → inverted"

# ============================================================
# Build arguments
# ============================================================

ABS_MAP_STRING=""
KEY_MAP_STRING=""

if ((${#EVDEV_ABSMAP[@]} > 0)); then
    IFS=,
    ABS_MAP_STRING="${EVDEV_ABSMAP[*]}"
    unset IFS
fi

if ((${#EVDEV_KEYMAP[@]} > 0)); then
    IFS=,
    KEY_MAP_STRING="${EVDEV_KEYMAP[*]}"
    unset IFS
fi

# ============================================================
# Start Xbox 360 emulation
# ============================================================

echo
echo "============================================================"
echo " STARTING XBOX 360 EMULATION"
echo "============================================================"
echo
echo "Physical:"
echo "  $EVENT_DEVICE"
echo
echo "Virtual:"
echo "  Microsoft X-Box 360 pad"
echo
echo "Press Ctrl+C to stop."
echo

XBOXDRV_ARGS=(
    --evdev "$EVENT_DEVICE"
    --mimic-xpad
)

# Physical -> Xbox axis mapping.
if [[ -n "$ABS_MAP_STRING" ]]; then
    XBOXDRV_ARGS+=(
        --evdev-absmap "$ABS_MAP_STRING"
    )
fi

# Physical -> Xbox button mapping.
if [[ -n "$KEY_MAP_STRING" ]]; then
    XBOXDRV_ARGS+=(
        --evdev-keymap "$KEY_MAP_STRING"
    )
fi

# ============================================================
# IMPORTANT:
# ============================================================
#
# The DragonRise controller reports its Y axes in the opposite
# direction from the Xbox 360 convention.
#
# Therefore:
#
#   physical UP    -> negative/positive source direction
#   Xbox UP        -> opposite direction
#
# xboxdrv supports inversion using --axismap:
#
#   -Y1=Y1
#   -Y2=Y2
#
# This does NOT modify the generated controller-map.conf.
# It is an output-side correction.
# ============================================================

XBOXDRV_ARGS+=(
    --axismap "-Y1=Y1,-Y2=Y2"
)

exec xboxdrv "${XBOXDRV_ARGS[@]}"