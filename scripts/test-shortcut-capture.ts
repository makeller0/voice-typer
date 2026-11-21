import assert from "node:assert/strict";
import { mock } from "bun:test";

const cleanups: (() => void)[] = [];
let capturing = true;
mock.module("react", () => ({
  useCallback: (callback: unknown) => callback,
  useRef: (current: unknown) => ({ current }),
  useState: (initial: unknown) => [
    typeof initial === "boolean" ? capturing : initial,
    () => {},
  ],
  useEffect: (effect: () => void | (() => void)) => {
    const cleanup = effect();
    if (cleanup) cleanups.push(cleanup);
  },
}));

let onNativeEvent: (event: { payload: unknown }) => void;
const saved: [string, string][] = [];
mock.module("@tauri-apps/api/event", () => ({
  listen: async (_name: string, callback: typeof onNativeEvent) => {
    onNativeEvent = callback;
    return () => {};
  },
}));
mock.module("@tauri-apps/plugin-opener", () => ({ openUrl: async () => {} }));
mock.module("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
mock.module("../src/stores/noticeStore", () => ({ notify: () => {} }));
mock.module("../src/bindings", () => ({ commands: {} }));
mock.module("../src/components/SecureInputWarning", () => ({
  SECURE_INPUT_HELP_URL: "",
}));
mock.module("../src/hooks/useSettings", () => ({
  useSettings: () => ({
    getSetting: () => "handy_keys",
    updateBinding: async (id: string, binding: string) => {
      saved.push([id, binding]);
    },
  }),
}));

const windowTarget = new EventTarget();
const documentTarget = new EventTarget();
Object.assign(globalThis, {
  window: windowTarget,
  document: documentTarget,
});
const { usePressAway } = await import("../src/hooks/usePressAway");
const { useShortcutCapture } = await import("../src/hooks/useShortcutCapture");

const mouseEvent = (type: string, button: number) => {
  const event = new Event(type, { cancelable: true });
  Object.defineProperty(event, "button", { value: button });
  return event;
};
const cleanup = () => {
  for (const dispose of cleanups.splice(0).reverse()) dispose();
};

let cancelled = 0;
const outside = { current: { contains: () => false } };
usePressAway(true, outside as never, () => cancelled++, {
  ignoreSideButtons: true,
});
for (const button of [3, 4]) {
  documentTarget.dispatchEvent(mouseEvent("pointerdown", button));
  assert.equal(cancelled, 0, "side buttons must not cancel capture");
}
documentTarget.dispatchEvent(mouseEvent("pointerdown", 0));
assert.equal(cancelled, 1, "left click outside still cancels");
cleanup();

usePressAway(true, outside as never, () => cancelled++);
documentTarget.dispatchEvent(mouseEvent("pointerdown", 4));
assert.equal(cancelled, 2, "other press-away users keep their behavior");
cleanup();

usePressAway(false, outside as never, () => cancelled++);
documentTarget.dispatchEvent(mouseEvent("pointerdown", 0));
assert.equal(cancelled, 2, "inactive press-away installs no listener");
cleanup();

for (const [button, binding] of [
  [3, "MouseX1"],
  [4, "MouseX2"],
] as const) {
  saved.length = 0;
  useShortcutCapture("transcribe_mouse", "windows");
  for (const type of ["mousedown", "mouseup", "auxclick"]) {
    const event = mouseEvent(type, button);
    windowTarget.dispatchEvent(event);
    assert.equal(event.defaultPrevented, true, `${type} navigation blocked`);
    const leftClick = mouseEvent(type, 0);
    windowTarget.dispatchEvent(leftClick);
    assert.equal(leftClick.defaultPrevented, false);
  }
  const emit = (is_key_down: boolean) =>
    onNativeEvent({
      payload: {
        modifiers: [],
        key: binding.toLowerCase(),
        hotkey_string: binding,
        is_key_down,
      },
    });
  emit(true);
  assert.deepEqual(saved, [], "button down must not save");
  emit(false);
  await Promise.resolve();
  assert.deepEqual(saved, [["transcribe_mouse", binding]], "release saves");
  cleanup();
  const event = mouseEvent("mouseup", button);
  windowTarget.dispatchEvent(event);
  assert.equal(event.defaultPrevented, false, "cleanup restores navigation");
}

capturing = false;
useShortcutCapture("transcribe_mouse", "windows");
const inactiveClick = mouseEvent("auxclick", 4);
windowTarget.dispatchEvent(inactiveClick);
assert.equal(inactiveClick.defaultPrevented, false);
cleanup();
console.log("shortcut capture: all assertions passed");
