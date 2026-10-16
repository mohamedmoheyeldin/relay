"""Key names used by the computer toolset (`key`, `hold_key`, click/scroll modifiers).

Accepts xdotool-style names ("Return", "Page_Down", "super") as Cloud AI writes them, plus
common aliases ("enter", "pgdn", "win"), and `+`-joined chords ("ctrl+shift+t").
Single printable characters are resolved to virtual keys by the backend at send time,
because that depends on the active keyboard layout.
"""

from __future__ import annotations

from dataclasses import dataclass

# Virtual-key codes (winuser.h)
VK_BACK = 0x08
VK_TAB = 0x09
VK_RETURN = 0x0D
VK_SHIFT = 0x10
VK_CONTROL = 0x11
VK_MENU = 0x12  # alt
VK_PAUSE = 0x13
VK_CAPITAL = 0x14
VK_ESCAPE = 0x1B
VK_SPACE = 0x20
VK_PRIOR = 0x21
VK_NEXT = 0x22
VK_END = 0x23
VK_HOME = 0x24
VK_LEFT = 0x25
VK_UP = 0x26
VK_RIGHT = 0x27
VK_DOWN = 0x28
VK_SNAPSHOT = 0x2C
VK_INSERT = 0x2D
VK_DELETE = 0x2E
VK_LWIN = 0x5B
VK_RWIN = 0x5C
VK_APPS = 0x5D
VK_NUMPAD0 = 0x60
VK_MULTIPLY = 0x6A
VK_ADD = 0x6B
VK_SUBTRACT = 0x6D
VK_DECIMAL = 0x6E
VK_DIVIDE = 0x6F
VK_F1 = 0x70
VK_NUMLOCK = 0x90
VK_SCROLL = 0x91
VK_LSHIFT = 0xA0
VK_RSHIFT = 0xA1
VK_LCONTROL = 0xA2
VK_RCONTROL = 0xA3
VK_LMENU = 0xA4
VK_RMENU = 0xA5
VK_VOLUME_MUTE = 0xAD
VK_VOLUME_DOWN = 0xAE
VK_VOLUME_UP = 0xAF
VK_MEDIA_NEXT_TRACK = 0xB0
VK_MEDIA_PREV_TRACK = 0xB1
VK_MEDIA_PLAY_PAUSE = 0xB3

# Keys that need KEYEVENTF_EXTENDEDKEY so they are not read as their numpad twins.
EXTENDED_VKS = frozenset(
    {
        VK_PRIOR,
        VK_NEXT,
        VK_END,
        VK_HOME,
        VK_LEFT,
        VK_UP,
        VK_RIGHT,
        VK_DOWN,
        VK_SNAPSHOT,
        VK_INSERT,
        VK_DELETE,
        VK_LWIN,
        VK_RWIN,
        VK_APPS,
        VK_DIVIDE,
        VK_NUMLOCK,
        VK_RCONTROL,
        VK_RMENU,
    }
)

MODIFIER_VKS = {
    "shift": VK_SHIFT,
    "ctrl": VK_CONTROL,
    "alt": VK_MENU,
    "super": VK_LWIN,
}

_MODIFIER_ALIASES = {
    "shift": "shift",
    "shift_l": "shift",
    "ctrl": "ctrl",
    "control": "ctrl",
    "control_l": "ctrl",
    "ctl": "ctrl",
    "alt": "alt",
    "alt_l": "alt",
    "option": "alt",
    "menu_alt": "alt",
    "super": "super",
    "super_l": "super",
    "win": "super",
    "windows": "super",
    "meta": "super",
    "cmd": "super",
    "command": "super",
}

_NAMED: dict[str, int] = {
    "return": VK_RETURN,
    "enter": VK_RETURN,
    "kp_enter": VK_RETURN,
    "tab": VK_TAB,
    "escape": VK_ESCAPE,
    "esc": VK_ESCAPE,
    "backspace": VK_BACK,
    "back_space": VK_BACK,
    "delete": VK_DELETE,
    "del": VK_DELETE,
    "insert": VK_INSERT,
    "ins": VK_INSERT,
    "home": VK_HOME,
    "end": VK_END,
    "page_up": VK_PRIOR,
    "pageup": VK_PRIOR,
    "pgup": VK_PRIOR,
    "prior": VK_PRIOR,
    "page_down": VK_NEXT,
    "pagedown": VK_NEXT,
    "pgdn": VK_NEXT,
    "next": VK_NEXT,
    "up": VK_UP,
    "down": VK_DOWN,
    "left": VK_LEFT,
    "right": VK_RIGHT,
    "space": VK_SPACE,
    "caps_lock": VK_CAPITAL,
    "capslock": VK_CAPITAL,
    "num_lock": VK_NUMLOCK,
    "numlock": VK_NUMLOCK,
    "scroll_lock": VK_SCROLL,
    "print": VK_SNAPSHOT,
    "printscreen": VK_SNAPSHOT,
    "print_screen": VK_SNAPSHOT,
    "pause": VK_PAUSE,
    "menu": VK_APPS,
    "apps": VK_APPS,
    "shift_r": VK_RSHIFT,
    "control_r": VK_RCONTROL,
    "ctrl_r": VK_RCONTROL,
    "alt_r": VK_RMENU,
    "super_r": VK_RWIN,
    "kp_add": VK_ADD,
    "kp_subtract": VK_SUBTRACT,
    "kp_multiply": VK_MULTIPLY,
    "kp_divide": VK_DIVIDE,
    "kp_decimal": VK_DECIMAL,
    "audiomute": VK_VOLUME_MUTE,
    "xf86audiomute": VK_VOLUME_MUTE,
    "audiolowervolume": VK_VOLUME_DOWN,
    "xf86audiolowervolume": VK_VOLUME_DOWN,
    "audioraisevolume": VK_VOLUME_UP,
    "xf86audioraisevolume": VK_VOLUME_UP,
    "audioplay": VK_MEDIA_PLAY_PAUSE,
    "xf86audioplay": VK_MEDIA_PLAY_PAUSE,
    "audionext": VK_MEDIA_NEXT_TRACK,
    "xf86audionext": VK_MEDIA_NEXT_TRACK,
    "audioprev": VK_MEDIA_PREV_TRACK,
    "xf86audioprev": VK_MEDIA_PREV_TRACK,
    # Punctuation spelled out (xdotool keysym names)
    "plus": -ord("+"),
    "minus": -ord("-"),
    "equal": -ord("="),
    "comma": -ord(","),
    "period": -ord("."),
    "slash": -ord("/"),
    "backslash": -ord("\\"),
    "semicolon": -ord(";"),
    "apostrophe": -ord("'"),
    "quotedbl": -ord('"'),
    "grave": -ord("`"),
    "bracketleft": -ord("["),
    "bracketright": -ord("]"),
    "underscore": -ord("_"),
}
for _n in range(1, 25):
    _NAMED[f"f{_n}"] = VK_F1 + _n - 1
for _n in range(10):
    _NAMED[f"kp_{_n}"] = VK_NUMPAD0 + _n


class KeyNameError(ValueError):
    """Unknown key name."""


@dataclass(frozen=True)
class Key:
    """A key to press: either a virtual-key code or a character resolved by layout."""

    vk: int | None = None
    char: str | None = None

    def __post_init__(self) -> None:
        if (self.vk is None) == (self.char is None):
            raise ValueError("Key needs exactly one of vk or char")

    @property
    def extended(self) -> bool:
        return self.vk in EXTENDED_VKS


@dataclass(frozen=True)
class Chord:
    modifiers: tuple[str, ...]  # canonical names from MODIFIER_VKS, in press order
    key: Key | None  # None for a modifier-only chord such as "super" or "ctrl+shift"


def normalize_modifier(name: str) -> str | None:
    return _MODIFIER_ALIASES.get(name.strip().lower())


def parse_key(name: str) -> Key:
    """Parse one key name (not a chord)."""
    if len(name) == 1:
        return Key(char=name)
    lowered = name.strip().lower()
    if lowered in _NAMED:
        code = _NAMED[lowered]
        return Key(char=chr(-code)) if code < 0 else Key(vk=code)
    if lowered in _MODIFIER_ALIASES:
        return Key(vk=MODIFIER_VKS[_MODIFIER_ALIASES[lowered]])
    raise KeyNameError(f"unknown key name {name!r}")


def _split_chord(text: str) -> list[str]:
    # "ctrl++" is ctrl + the plus key; a lone "+" is the plus key.
    if text == "+":
        return ["+"]
    parts = [*text[:-2].split("+"), "+"] if text.endswith("++") else text.split("+")
    if any(part == "" for part in parts):
        raise KeyNameError(f"empty key in {text!r}")
    return parts


def parse_chord(text: str) -> Chord:
    """Parse "ctrl+s", "alt+Tab", "Return", "super", "ctrl+shift"."""
    if not text or not text.strip():
        raise KeyNameError("empty key")
    parts = _split_chord(text.strip())
    modifiers: list[str] = []
    key: Key | None = None
    for i, part in enumerate(parts):
        mod = normalize_modifier(part)
        last = i == len(parts) - 1
        if mod is not None and not last:
            if mod not in modifiers:
                modifiers.append(mod)
            continue
        if not last:
            raise KeyNameError(f"{part!r} is not a modifier; only the last key in a chord may be")
        if mod is not None and not modifiers:
            # A bare modifier ("super") is pressed as a key on its own.
            return Chord((), Key(vk=MODIFIER_VKS[mod]))
        if mod is not None:
            if mod not in modifiers:
                modifiers.append(mod)
            return Chord(tuple(modifiers), None)
        key = parse_key(part)
    return Chord(tuple(modifiers), key)


def parse_modifiers(text: str | None) -> tuple[str, ...]:
    """Parse the optional `text` modifier argument of clicks and scroll ("ctrl+shift")."""
    if text is None or not text.strip():
        return ()
    result: list[str] = []
    for part in text.split("+"):
        mod = normalize_modifier(part)
        if mod is None:
            raise KeyNameError(f"{part!r} is not a modifier (use shift, ctrl, alt or super)")
        if mod not in result:
            result.append(mod)
    return tuple(result)
