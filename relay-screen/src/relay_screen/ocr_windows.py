"""Windows.Media.Ocr engine for ocr.py (built into Windows 10/11, no model download)."""

from __future__ import annotations

import asyncio
from typing import Any

from PIL import Image

from relay_screen import win32
from relay_screen.ocr import Line, Word
from relay_screen.scaling import Rect


class WinOcrEngine:
    def __init__(self) -> None:
        win32.require_windows()
        from winrt.windows.media.ocr import OcrEngine  # pyright: ignore[reportMissingImports]

        engine = OcrEngine.try_create_from_user_profile_languages()
        if engine is None:
            from winrt.windows.globalization import Language  # pyright: ignore[reportMissingImports]

            engine = OcrEngine.try_create_from_language(Language("en-US"))
        if engine is None:
            raise RuntimeError(
                "Windows OCR has no language installed. Add one with: "
                'Add-WindowsCapability -Online -Name "Language.OCR~~~en-US~0.0.1.0"'
            )
        self._engine: Any = engine
        self.max_dimension = int(OcrEngine.max_image_dimension)

    def recognize(self, image: Image.Image) -> list[Line]:
        from winrt.windows.graphics.imaging import (  # pyright: ignore[reportMissingImports]
            BitmapPixelFormat,
            SoftwareBitmap,
        )
        from winrt.windows.storage.streams import DataWriter  # pyright: ignore[reportMissingImports]

        rgba = image.convert("RGBA")
        writer = DataWriter()
        writer.write_bytes(rgba.tobytes())
        bitmap = SoftwareBitmap.create_copy_from_buffer(
            writer.detach_buffer(), BitmapPixelFormat.RGBA8, rgba.width, rgba.height
        )

        async def run() -> Any:
            return await self._engine.recognize_async(bitmap)

        result = asyncio.run(run())
        lines = []
        for line in result.lines:
            words = []
            for word in line.words:
                r = word.bounding_rect
                box = Rect(round(r.x), round(r.y), max(1, round(r.width)), max(1, round(r.height)))
                words.append(Word(word.text, box))
            if words:
                lines.append(Line(tuple(words)))
        return lines
