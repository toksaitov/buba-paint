import { useMobileNavStore } from "../mobile-nav-store";

beforeEach(() => {
  useMobileNavStore.setState({ isOpen: false });
});

test("starts closed", () => {
  expect(useMobileNavStore.getState().isOpen).toBe(false);
});

test("open sets isOpen to true", () => {
  useMobileNavStore.getState().open();
  expect(useMobileNavStore.getState().isOpen).toBe(true);
});

test("close sets isOpen to false", () => {
  useMobileNavStore.setState({ isOpen: true });
  useMobileNavStore.getState().close();
  expect(useMobileNavStore.getState().isOpen).toBe(false);
});

test("toggle flips isOpen", () => {
  useMobileNavStore.getState().toggle();
  expect(useMobileNavStore.getState().isOpen).toBe(true);
  useMobileNavStore.getState().toggle();
  expect(useMobileNavStore.getState().isOpen).toBe(false);
});
