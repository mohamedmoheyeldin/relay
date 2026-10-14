"""Find text on screen with OCR (architecture §4.1, M1-08).

Windows' built-in OCR (Windows.Media.Ocr) reads the active display or a region; this
module tiles large captures to the engine's size limit, maps word boxes back to
screenshot pixels of the active display and matches a query word or phrase, fuzzy by
default. The engine itself is behind a small protocol so this logic is tested on any
OS; ocr_windows.py is the real one.
"""

from __future__ import annotations

import time
from dataclasses import dataclass
from difflib import SequenceMatcher
from typing import Protocol

from PIL import Image

from relay_screen.capture import Grabber
from relay_screen.errors import ActionError
from relay_screen.scaling import Rect, ScreenshotGeometry

TILE_OVERLAP = 160  # px of overlap between tiles so words on a seam are whole in one
SMALL_REGION_UPSCALE = 2.0  # OCR reads small UI text better when it is bigger
FUZZY_MIN = 0.8
MAX_MATCHES = 10
MAX_LINES = 150  # when listing all text


@dataclass(frozen=True)
class Word:
    text: str
    rect: Rect  # in the coordinate space of whatever produced it


@dataclass(frozen=True)
class Line:
    words: tuple[Word, ...]

    @property
    def text(self) -> str:
        return " ".join(w.text for w in self.words)


class OcrEngine(Protocol):
    max_dimension: int

    def recognize(self, image: Image.Image) -> list[Line]:
        """Lines with word boxes in the image's pixels."""
        ...


def _union(rects: list[Rect]) -> Rect:
    left = min(r.left for r in rects)
    top = min(r.top for r in rects)
    right = max(r.right for r in rects)
    bottom = max(r.bottom for r in rects)
    return Rect(left, top, right - left, bottom - top)


def _shift(line: Line, dx: float, dy: float, scale: float) -> Line:
    """Image px of a (scaled) tile -> physical px."""
    words = []
    for w in line.words:
        r = w.rect
        words.append(
            Word(
                w.text,
                Rect(
                    round(dx + r.left / scale),
                    round(dy + r.top / scale),
                    max(1, round(r.width / scale)),
                    max(1, round(r.height / scale)),
                ),
            )
        )
    return Line(tuple(words))


def tiles(width: int, height: int, limit: int, overlap: int = TILE_OVERLAP) -> list[Rect]:
    """Split an area into tiles no larger than ``limit`` with some overlap."""

    def spans(total: int) -> list[tuple[int, int]]:
        if total <= limit:
            return [(0, total)]
        step = limit - overlap
        out = []
        start = 0
        while True:
            end = min(start + limit, total)
            out.append((start, end))
            if end == total:
                return out
            start += step

    return [Rect(x0, y0, x1 - x0, y1 - y0) for y0, y1 in spans(height) for x0, x1 in spans(width)]


@dataclass
class Match:
    text: str
    rect: Rect  # physical
    score: float
    line: str


def _score(query: str, candidate: str) -> float:
    q, c = query.casefold(), candidate.casefold()
    if q == c:
        return 1.0
    return SequenceMatcher(None, q, c).ratio()


def match(lines: list[Line], query: str, fuzzy: bool = True) -> list[Match]:
    """Best spans of consecutive words matching the query, best first."""
    q = " ".join(query.split())
    n = max(1, len(q.split(" ")))
    found: list[Match] = []
    for line in lines:
        words = line.words
        for size in {n, n + 1, max(1, n - 1)}:  # OCR may split or merge a word
            for i in range(0, max(0, len(words) - size) + 1):
                span = words[i : i + size]
                if not span:
                    continue
                text = " ".join(w.text for w in span)
                score = _score(q, text)
                contains = q.casefold() in text.casefold() and size == n
                if contains:
                    score = max(score, 0.95)
                if score >= (FUZZY_MIN if fuzzy else 1.0) or (contains and not fuzzy):
                    found.append(Match(text, _union([w.rect for w in span]), score, line.text))
    found.sort(key=lambda m: (-m.score, m.rect.top, m.rect.left))
    unique: list[Match] = []
    for m in found:  # drop overlapping duplicates of a better match
        if all(m.rect.intersect(u.rect) is None for u in unique):
            unique.append(m)
    return unique[:MAX_MATCHES]


class OcrService:
    def __init__(self, engine: OcrEngine, grabber: Grabber) -> None:
        self.engine = engine
        self.grabber = grabber
        self.last_seconds = 0.0

    def read(self, area: Rect, upscale: float = 1.0) -> list[Line]:
        """OCR a physical-pixel area; returns lines with physical-pixel boxes."""
        start = time.monotonic()
        image = self.grabber.grab(area).convert("RGB")
        limit = self.engine.max_dimension
        if upscale > 1.0:
            upscale = min(upscale, limit / max(image.width, image.height))
        upscale = max(1.0, upscale)
        lines: list[Line] = []
        for tile in tiles(image.width, image.height, int(limit / upscale)):
            crop = image.crop((tile.left, tile.top, tile.right, tile.bottom))
            if upscale > 1.0:
                size = (round(crop.width * upscale), round(crop.height * upscale))
                crop = crop.resize(size, Image.Resampling.LANCZOS)
            for line in self.engine.recognize(crop):
                shifted = _shift(line, area.left + tile.left, area.top + tile.top, upscale)
                if _owned(shifted, tile, area, image.width, image.height):
                    lines.append(shifted)
        lines.sort(key=lambda ln: (ln.words[0].rect.top // 8, ln.words[0].rect.left))
        self.last_seconds = time.monotonic() - start
        return lines

    def find_text(
        self,
        geometry: ScreenshotGeometry,
        text: str | None = None,
        region: tuple[int, int, int, int] | None = None,
        fuzzy: bool = True,
    ) -> str:
        if region is not None:
            try:
                area = geometry.region_to_physical(*region)
            except ValueError as exc:
                raise ActionError(f"region: {exc}") from exc
            small = max(area.width, area.height) < 1200
            lines = self.read(area, SMALL_REGION_UPSCALE if small else 1.0)
        else:
            lines = self.read(geometry.display)
        took = f"(OCR {self.last_seconds:.2f} s)"
        if not lines:
            return f"No text found {took}."
        if text is None or not text.strip():
            shown = lines[:MAX_LINES]
            out = [f"{len(lines)} line(s) of text {took}:"]
            out += [
                f'"{ln.text}" {_box(geometry, _union([w.rect for w in ln.words]))}' for ln in shown
            ]
            if len(lines) > MAX_LINES:
                out.append(f"… {len(lines) - MAX_LINES} more lines; pass text or a region.")
            return "\n".join(out)
        matches = match(lines, text, fuzzy)
        if not matches:
            return f"No match for {text!r} {took}."
        out = [f"{len(matches)} match(es) for {text!r}, best first {took}:"]
        for m in matches:
            score = "" if m.score >= 0.999 else f" score {m.score:.2f}"
            line = f' in "{m.line}"' if m.line != m.text else ""
            out.append(f'"{m.text}"{score} {_box(geometry, m.rect)}{line}')
        return "\n".join(out)


def _owned(line: Line, tile: Rect, area: Rect, width: int, height: int) -> bool:
    """Keep a line only in the tile that owns its center, so overlaps don't repeat it."""
    box = _union([w.rect for w in line.words])
    cx = box.left + box.width / 2 - area.left
    cy = box.top + box.height / 2 - area.top
    half = TILE_OVERLAP / 2
    left = tile.left + (half if tile.left > 0 else 0)
    right = tile.right - (half if tile.right < width else 0)
    top = tile.top + (half if tile.top > 0 else 0)
    bottom = tile.bottom - (half if tile.bottom < height else 0)
    return left <= cx <= right and top <= cy <= bottom


def _box(geometry: ScreenshotGeometry, rect: Rect) -> str:
    cx, cy = rect.left + rect.width // 2, rect.top + rect.height // 2
    sx, sy = geometry.to_screenshot(cx, cy)
    x0, y0 = geometry.to_screenshot(rect.left, rect.top)
    x1, y1 = geometry.to_screenshot(rect.right - 1, rect.bottom - 1)
    return f"center ({sx}, {sy}) box [{x0}, {y0}, {x1}, {y1}]"
