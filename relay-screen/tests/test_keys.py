import pytest

from relay_screen.input import keys
from relay_screen.input.keys import Chord, Key, KeyNameError, parse_chord, parse_modifiers


@pytest.mark.parametrize(
    ("text", "expected"),
    [
        ("Return", Chord((), Key(vk=keys.VK_RETURN))),
        ("enter", Chord((), Key(vk=keys.VK_RETURN))),
        ("ctrl+s", Chord(("ctrl",), Key(char="s"))),
        ("alt+Tab", Chord(("alt",), Key(vk=keys.VK_TAB))),
        ("ctrl+shift+t", Chord(("ctrl", "shift"), Key(char="t"))),
        ("super", Chord((), Key(vk=keys.VK_LWIN))),
        ("Page_Down", Chord((), Key(vk=keys.VK_NEXT))),
        ("F5", Chord((), Key(vk=keys.VK_F1 + 4))),
        ("ctrl++", Chord(("ctrl",), Key(char="+"))),
        ("+", Chord((), Key(char="+"))),
        ("ctrl+plus", Chord(("ctrl",), Key(char="+"))),
        ("ctrl+shift", Chord(("ctrl", "shift"), None)),
        ("Control_L+a", Chord(("ctrl",), Key(char="a"))),
        ("cmd+space", Chord(("super",), Key(vk=keys.VK_SPACE))),
        ("A", Chord((), Key(char="A"))),
    ],
)
def test_parse_chord(text, expected):
    assert parse_chord(text) == expected


@pytest.mark.parametrize("bad", ["", "  ", "ctrl+", "a+b", "nosuchkey", "ctrl++a"])
def test_parse_chord_rejects(bad):
    with pytest.raises(KeyNameError):
        parse_chord(bad)


def test_extended_flag():
    assert parse_chord("Delete").key.extended  # type: ignore[union-attr]
    assert not parse_chord("Return").key.extended  # type: ignore[union-attr]


def test_parse_modifiers():
    assert parse_modifiers(None) == ()
    assert parse_modifiers("ctrl+shift") == ("ctrl", "shift")
    assert parse_modifiers("win") == ("super",)
    with pytest.raises(KeyNameError):
        parse_modifiers("ctrl+a")
