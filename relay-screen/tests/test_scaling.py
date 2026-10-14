import pytest

from relay_screen.scaling import (
    ImageBudget,
    Rect,
    ScreenshotGeometry,
    fit_size,
    normalize_absolute,
)

DEFAULT = ImageBudget()


@pytest.mark.parametrize(
    ("size", "expected"),
    [
        ((3440, 1440), (1568, 656)),
        ((2560, 720), (1568, 441)),
        ((1920, 1080), (1568, 882)),
        ((1280, 720), (1280, 720)),  # never upscales
        ((1440, 3440), (656, 1568)),  # portrait
    ],
)
def test_fit_size_default_budget(size, expected):
    width, height = size
    assert fit_size(width, height, DEFAULT) == expected


def test_fit_size_respects_pixel_cap():
    budget = ImageBudget(long_edge=2576)
    w, h = fit_size(3840, 2160, budget)
    assert w <= 2576 and w * h <= 3_750_000
    assert abs(w / h - 3840 / 2160) < 0.01


@pytest.mark.parametrize("bad", [10, 5000])
def test_budget_rejects_out_of_range(bad):
    with pytest.raises(ValueError):
        ImageBudget(long_edge=bad)


DISPLAYS = [
    Rect(0, 0, 3440, 1440),
    Rect(0, 1440, 3440, 1440),  # below primary
    Rect(-2560, 360, 2560, 720),  # left of primary, negative x
    Rect(0, 0, 1920, 1080),
    # Physical sizes of common panels at 125% and 150% scale (rects are physical px).
    Rect(1920, 0, 2560, 1440),
    Rect(-3840, -2160, 3840, 2160),
]


@pytest.mark.parametrize("display", DISPLAYS)
def test_round_trip_physical_error_at_most_one_pixel(display):
    geometry = ScreenshotGeometry.for_display(display, DEFAULT)
    step_x = max(1, display.width // 97)
    step_y = max(1, display.height // 53)
    for px in range(display.left, display.right, step_x):
        for py in range(display.top, display.bottom, step_y):
            sx, sy = geometry.to_screenshot(px, py)
            back_x, back_y = geometry.to_physical(sx, sy)
            assert abs(back_x - px) <= 1 / geometry.scale_x + 1
            assert abs(back_y - py) <= 1 / geometry.scale_y + 1
            assert display.contains(back_x, back_y)


@pytest.mark.parametrize("display", DISPLAYS)
def test_screenshot_corners_stay_on_display(display):
    geometry = ScreenshotGeometry.for_display(display, DEFAULT)
    for sx, sy in [
        (0, 0),
        (geometry.width - 1, 0),
        (0, geometry.height - 1),
        (geometry.width - 1, geometry.height - 1),
    ]:
        assert display.contains(*geometry.to_physical(sx, sy))
    x, y = geometry.to_physical(0, 0)
    assert 0 <= x - display.left < 1 / geometry.scale_x
    assert 0 <= y - display.top < 1 / geometry.scale_y


def test_screenshot_pixel_maps_to_its_centre():
    geometry = ScreenshotGeometry.for_display(Rect(0, 0, 3440, 1440), DEFAULT)
    # 3440 / 1568 ~= 2.19 physical px per screenshot px; pixel 100 covers ~219..221.
    x, _ = geometry.to_physical(100, 0)
    assert 219 <= x <= 221


def test_full_size_geometry_is_identity():
    geometry = ScreenshotGeometry.for_display(Rect(-1280, 0, 1280, 720), DEFAULT)
    assert (geometry.width, geometry.height) == (1280, 720)
    assert geometry.to_physical(10, 20) == (-1270, 20)
    assert geometry.to_screenshot(-1270, 20) == (10, 20)


def test_check_point_rejects_outside():
    geometry = ScreenshotGeometry.for_display(Rect(0, 0, 3440, 1440), DEFAULT)
    geometry.check_point(1567, 655)
    with pytest.raises(ValueError, match="outside the screenshot"):
        geometry.check_point(1568, 10)
    with pytest.raises(ValueError):
        geometry.check_point(-1, 10)


def test_region_to_physical_covers_region():
    display = Rect(0, 1440, 3440, 1440)
    geometry = ScreenshotGeometry.for_display(display, DEFAULT)
    rect = geometry.region_to_physical(100, 50, 299, 149)
    assert rect.left <= geometry.to_physical(100, 50)[0]
    assert rect.top <= geometry.to_physical(100, 50)[1]
    assert rect.right >= geometry.to_physical(299, 149)[0]
    assert rect.bottom >= geometry.to_physical(299, 149)[1]
    assert display.intersect(rect) == rect
    # 200x100 screenshot px of a 2.19x display is ~439x219 physical px.
    assert 435 <= rect.width <= 442 and 216 <= rect.height <= 222


def test_region_validation():
    geometry = ScreenshotGeometry.for_display(Rect(0, 0, 1920, 1080), DEFAULT)
    with pytest.raises(ValueError):
        geometry.region_to_physical(50, 50, 10, 60)
    with pytest.raises(ValueError):
        geometry.region_to_physical(0, 0, 5000, 10)


def test_normalize_absolute_corners():
    virtual = Rect(-2560, 0, 6000, 2880)
    assert normalize_absolute(-2560, 0, virtual) == (0, 0)
    assert normalize_absolute(3439, 2879, virtual) == (65535, 65535)
    with pytest.raises(ValueError):
        normalize_absolute(3440, 0, virtual)


def test_rect_intersect():
    assert Rect(0, 0, 10, 10).intersect(Rect(5, 5, 10, 10)) == Rect(5, 5, 5, 5)
    assert Rect(0, 0, 10, 10).intersect(Rect(10, 0, 5, 5)) is None


@pytest.mark.parametrize("display", DISPLAYS)
def test_screenshot_round_trip_is_exact(display):
    """Clicking where Claude points and reading the position back gives the same pixel."""
    geometry = ScreenshotGeometry.for_display(display, DEFAULT)
    for sx in range(0, geometry.width, 7):
        for sy in range(0, geometry.height, 11):
            assert geometry.to_screenshot(*geometry.to_physical(sx, sy)) == (sx, sy)
