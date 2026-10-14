"""GPU facts from NVML: name, driver, CUDA version and free VRAM (D-006).

Loaded through ctypes so it needs no extra package; returns None without an NVIDIA GPU.
"""

from __future__ import annotations

import ctypes
import os
import sys
from dataclasses import dataclass

GIB = 1024**3


@dataclass(frozen=True)
class Gpu:
    name: str
    driver: str
    cuda: tuple[int, int]
    """The newest CUDA version the driver supports."""
    total: int
    free: int


class _Memory(ctypes.Structure):
    _fields_ = [
        ("total", ctypes.c_ulonglong),
        ("free", ctypes.c_ulonglong),
        ("used", ctypes.c_ulonglong),
    ]


def _library() -> ctypes.CDLL | None:
    if sys.platform == "win32":
        names = [
            "nvml.dll",
            os.path.join(
                os.environ.get("PROGRAMFILES", r"C:\Program Files"),
                "NVIDIA Corporation",
                "NVSMI",
                "nvml.dll",
            ),
        ]
    else:
        names = ["libnvidia-ml.so.1"]
    for name in names:
        try:
            return ctypes.CDLL(name)
        except OSError:
            continue
    return None


def query(index: int = 0) -> Gpu | None:
    """The GPU at `index` now, or None if there is no NVIDIA driver or GPU."""
    nvml = _library()
    if nvml is None or nvml.nvmlInit_v2() != 0:
        return None
    try:
        count = ctypes.c_uint()
        if nvml.nvmlDeviceGetCount_v2(ctypes.byref(count)) != 0 or count.value <= index:
            return None
        handle = ctypes.c_void_p()
        if nvml.nvmlDeviceGetHandleByIndex_v2(index, ctypes.byref(handle)) != 0:
            return None
        name = ctypes.create_string_buffer(96)
        nvml.nvmlDeviceGetName(handle, name, 96)
        driver = ctypes.create_string_buffer(80)
        nvml.nvmlSystemGetDriverVersion(driver, 80)
        cuda = ctypes.c_int()
        nvml.nvmlSystemGetCudaDriverVersion_v2(ctypes.byref(cuda))
        memory = _Memory()
        if nvml.nvmlDeviceGetMemoryInfo(handle, ctypes.byref(memory)) != 0:
            return None
        return Gpu(
            name.value.decode(errors="replace"),
            driver.value.decode(errors="replace"),
            (cuda.value // 1000, cuda.value % 1000 // 10),
            memory.total,
            memory.free,
        )
    finally:
        nvml.nvmlShutdown()


def free_vram() -> int | None:
    gpu = query()
    return gpu.free if gpu else None


def gib(n: int) -> str:
    return f"{n / GIB:.1f} GB"
