import threading

from relay_screen.lifetime import exit_with_parent


def test_exits_once_the_parent_is_gone() -> None:
    parents = iter([42, 42, 42, 1])
    exited = threading.Event()
    codes: list[int] = []

    def leave(code: int) -> None:
        codes.append(code)
        exited.set()

    thread = exit_with_parent(
        exit=leave, getppid=lambda: next(parents), sleep=lambda s: None, windows=False
    )
    assert thread is not None
    assert exited.wait(2)
    assert codes == [0]


def test_no_watch_without_a_parent() -> None:
    assert exit_with_parent(exit=lambda c: None, getppid=lambda: 1, windows=False) is None
