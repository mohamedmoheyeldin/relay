from relay_screen import capture
from relay_screen.scaling import ImageBudget, Rect, ScreenshotGeometry
from tests.conftest import GradientGrabber

LOWER = Rect(0, 1440, 3440, 1440)


def geometry(rect=LOWER):
    return ScreenshotGeometry.for_display(rect, ImageBudget())


def test_screenshot_resized_to_budget():
    grabber = GradientGrabber()
    image = capture.screenshot(grabber, geometry())
    assert image.size == (1568, 656)
    assert grabber.grabs == [Rect(0, 1440, 3440, 1440)]


def test_zoom_grabs_physical_region_and_fits_box():
    grabber = GradientGrabber()
    g = geometry()
    image = capture.zoom(grabber, g, (100, 50, 299, 149))
    grabbed = grabber.grabs[0]
    assert grabbed.top >= 1440 and 435 <= grabbed.width <= 442
    # A 2:1 region fits the 1568x656 box by height.
    assert (
        image.size[1] == 656
        and abs(image.size[0] / image.size[1] - grabbed.width / grabbed.height) < 0.01
    )


def test_zoom_of_wide_region_fits_by_width():
    grabber = GradientGrabber()
    image = capture.zoom(grabber, geometry(), (0, 0, 1567, 99))
    assert image.size[0] == 1568 and image.size[1] < 656


def test_encode_png_roundtrip():
    grabber = GradientGrabber()
    data = capture.encode_png(capture.screenshot(grabber, geometry(Rect(0, 0, 800, 600))))
    assert data.startswith(b"\x89PNG")
