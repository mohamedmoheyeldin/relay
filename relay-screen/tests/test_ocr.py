from __future__ import annotations

import pytest
from PIL import Image

from relay_screen.actions import Computer
from relay_screen.errors import ActionError
from relay_screen.ocr import Line, OcrService, Word, match, tiles
from relay_screen.scaling import Rect
from relay_screen.state import ScreenState
from tests.conftest import GradientGrabber


def line(*words: tuple[str, int, int, int, int]) -> Line:
    return Line(tuple(Word(t, Rect(x, y, w, h)) for t, x, y, w, h in words))


class FakeEngine:
    """Returns fixed lines in physical px of the display, clipped to each tile."""

    max_dimension = 2600

    def __init__(self, lines: list[Line]) -> None:
        self.lines = lines
        self.calls: list[tuple[int, int]] = []
        self.origin: tuple[int, int] = (0, 0)
        self.scale = 1.0

    def recognize(self, image: Image.Image) -> list[Line]:
        self.calls.append(image.size)
        ox, oy = self.origin
        out = []
        for ln in self.lines:
            words = []
            for w in ln.words:
                r = w.rect
                x = (r.left - ox) * self.scale
                y = (r.top - oy) * self.scale
                inside = x >= 0 and x + r.width * self.scale <= image.width
                if inside and y >= 0 and y + r.height * self.scale <= image.height:
                    words.append(
                        Word(
                            w.text,
                            Rect(
                                round(x),
                                round(y),
                                round(r.width * self.scale),
                                round(r.height * self.scale),
                            ),
                        )
                    )
            if words:
                out.append(Line(tuple(words)))
        return out


def test_tiles_cover_wide_displays_within_the_limit():
    parts = tiles(3440, 1440, 2600)
    assert all(t.width <= 2600 and t.height <= 2600 for t in parts)
    assert parts[0].left == 0 and parts[-1].right == 3440
    assert parts[1].left < parts[0].right  # overlap
    assert tiles(1000, 500, 2600) == [Rect(0, 0, 1000, 500)]


def test_match_exact_fuzzy_and_phrases():
    lines = [
        line(("File", 10, 10, 30, 12), ("Edit", 50, 10, 30, 12), ("View", 90, 10, 34, 12)),
        line(("Save", 500, 600, 40, 14), ("Cancel", 560, 600, 50, 14)),
        line(("Don't", 300, 600, 40, 14), ("Save", 345, 600, 40, 14)),
    ]
    best = match(lines, "save")
    assert [m.text for m in best[:2]] == ["Save", "Save"]
    assert best[0].score == 1.0
    phrase = match(lines, "don't save")
    assert phrase[0].text == "Don't Save" and phrase[0].rect == Rect(300, 600, 85, 14)
    assert match(lines, "Cancle")[0].text == "Cancel"  # OCR-style typo still matches
    assert match(lines, "Cancle", fuzzy=False) == []
    assert match(lines, "zebra") == []


def test_wide_display_is_tiled_without_duplicates(owner_displays):
    # A word inside the overlap of both tiles must be reported once.
    lines = [line(("Seam", 2500, 700, 60, 20)), line(("Save", 3000, 1300, 60, 20))]
    engine = FakeEngine(lines)
    parts = iter(tiles(3440, 1440, 2600))

    original = engine.recognize

    def recognize(image: Image.Image) -> list[Line]:
        tile = next(parts)
        engine.origin = (tile.left, tile.top)
        return original(image)

    engine.recognize = recognize  # type: ignore[method-assign]
    service = OcrService(engine, GradientGrabber())
    geometry = ScreenState(lambda: owner_displays, lambda ds: None).geometry()
    text = service.find_text(geometry)
    assert all(w <= 2600 for w, _ in engine.calls) and len(engine.calls) == 2
    assert text.count('"Seam"') == 1 and text.count('"Save"') == 1


class OneShotEngine:
    max_dimension = 2600

    def __init__(self, lines: list[Line]) -> None:
        self.lines = lines
        self.sizes: list[tuple[int, int]] = []

    def recognize(self, image: Image.Image) -> list[Line]:
        self.sizes.append(image.size)
        return self.lines if len(self.sizes) == 1 else []


def test_find_text_reports_screenshot_coordinates(owner_displays):
    engine = OneShotEngine([line(("Save", 2200, 1000, 88, 22), ("Cancel", 2320, 1000, 110, 22))])
    service = OcrService(engine, GradientGrabber())
    geometry = ScreenState(lambda: owner_displays, lambda ds: None).geometry()
    text = service.find_text(geometry, "save")
    first = text.splitlines()[1]
    cx, cy = geometry.to_screenshot(2244, 1011)
    assert first.startswith(f'"Save" center ({cx}, {cy})')
    assert 'in "Save Cancel"' in first
    engine.sizes.clear()
    listing = service.find_text(geometry)
    assert listing.splitlines()[0].startswith("1 line(s) of text")


def test_small_regions_are_upscaled(owner_displays):
    engine = OneShotEngine([line(("OK", 40, 20, 20, 10))])
    service = OcrService(engine, GradientGrabber())
    geometry = ScreenState(lambda: owner_displays, lambda ds: None).geometry()
    text = service.find_text(geometry, "ok", region=(100, 100, 200, 150))
    area = geometry.region_to_physical(100, 100, 200, 150)
    assert engine.sizes[0] == (area.width * 2, area.height * 2)
    # Word box (40, 20) in the 2x image is (20, 10) from the region's corner.
    cx, cy = geometry.to_screenshot(area.left + 20 + 5, area.top + 10 + 2)
    assert f"center ({cx}, {cy})" in text


def test_find_text_through_computer(owner_displays):
    engine = OneShotEngine([line(("Submit", 400, 300, 90, 24))])
    state = ScreenState(lambda: owner_displays, lambda ds: None)
    computer = Computer(state, GradientGrabber(), None, ocr=OcrService(engine, GradientGrabber()))
    assert '"Submit"' in (computer.run("find_text", {"text": "submit"}).text or "")
    with pytest.raises(ActionError, match="region"):
        computer.run("find_text", {"text": "x", "region": [1, 2, 3]})
    bare = Computer(state, GradientGrabber(), None)
    with pytest.raises(ActionError, match="OCR is not available"):
        bare.run("find_text", {"text": "x"})
