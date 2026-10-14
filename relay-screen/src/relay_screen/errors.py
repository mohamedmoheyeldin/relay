"""Errors whose message goes back to Claude as a tool error."""

from __future__ import annotations


class ActionError(ValueError):
    """An action failed or its input was invalid; the message goes back to Claude."""
