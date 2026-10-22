import { act, renderHook } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { PointerEvent } from "react";
import useAssetDrag from "./useAssetDrag";
it("moves on a short pointer gesture, copies with Ctrl, and ignores clicks and invalid destinations", () => {
  const target = document.createElement("div");
  target.dataset.dropFolder = "destination";
  Object.defineProperty(document, "elementFromPoint", {
    configurable: true,
    value: () => target,
  });
  const onDrop = vi.fn();
  const capture = vi.fn();
  const event = (x: number, ctrlKey = false) =>
    ({
      button: 0,
      pointerId: 1,
      clientX: x,
      clientY: 20,
      ctrlKey,
      currentTarget: { setPointerCapture: capture },
      preventDefault: vi.fn(),
    }) as unknown as PointerEvent;
  const item = { id: "source", folder: true, name: "Sources" };
  const { result } = renderHook(() =>
    useAssetDrag(false, (_, t) => t !== "source", onDrop),
  );
  act(() => {
    result.current.start(event(10), item);
    result.current.end(event(10));
  });
  expect(onDrop).not.toHaveBeenCalled();
  act(() => {
    result.current.start(event(10), item);
    result.current.move(event(40));
    result.current.end(event(40));
  });
  expect(onDrop).toHaveBeenLastCalledWith(item, "destination", false);
  act(() => {
    result.current.start(event(10), item);
    result.current.move(event(40, true));
    result.current.end(event(40, true));
  });
  expect(onDrop).toHaveBeenLastCalledWith(item, "destination", true);
  target.dataset.dropFolder = "source";
  act(() => {
    result.current.start(event(10), item);
    result.current.move(event(40));
    result.current.end(event(40));
  });
  expect(onDrop).toHaveBeenCalledTimes(2);
  expect(result.current.preview).toBeNull();
});
