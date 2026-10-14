"""The curated local model catalog behind Settings > Local AI (D-036, D-046)."""

from __future__ import annotations

from dataclasses import dataclass
from typing import Literal

Role = Literal["locate", "check", "both"]

ROLE_LABEL = {"locate": "where is it", "check": "check / describe", "both": "both"}


@dataclass(frozen=True)
class ModelSpec:
    id: str
    name: str
    role: Role
    repo: str | None
    """Hugging Face repo with GGUF + mmproj files. None until a trusted source is chosen."""
    quant: str = "Q4_K_M"
    context: int = 8192
    license: str = "Apache-2.0"


CATALOG: tuple[ModelSpec, ...] = (
    # GGUF sources for the GUI models are picked with the M1-11 benchmark; only
    # publisher-owned repos go here, so a lookalike repo can't serve weights.
    ModelSpec("ui-venus-1.5-8b", "UI-Venus-1.5-8B", "locate", None),
    ModelSpec("holo2-8b", "Holo2-8B", "both", None),
    ModelSpec("holo2-4b", "Holo2-4B", "both", None),
    ModelSpec(
        "qwen3-vl-8b-instruct", "Qwen3-VL-8B-Instruct", "check", "Qwen/Qwen3-VL-8B-Instruct-GGUF"
    ),
)

# D-036: the resident pair.
DEFAULT_LOCATE = "ui-venus-1.5-8b"
DEFAULT_CHECK = "qwen3-vl-8b-instruct"


def find(model_id: str) -> ModelSpec | None:
    key = model_id.lower()
    return next((m for m in CATALOG if m.id == key or m.name.lower() == key), None)


def custom(repo: str, quant: str) -> ModelSpec:
    """A model the owner adds by Hugging Face repo (D-046)."""
    model_id = repo.rsplit("/", 1)[-1].lower().removesuffix("-gguf")
    return ModelSpec(model_id, repo, "both", repo, quant=quant, license="see repo")
