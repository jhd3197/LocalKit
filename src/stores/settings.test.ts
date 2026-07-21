import { beforeEach, describe, expect, it } from "vitest";
import {
  TERMINAL_FONT_SIZE_DEFAULT,
  getOsNotificationsEnabled,
  getTerminalFontSize,
  getUpdateLastChecked,
  useSettings,
} from "./settings";

const set = (key: string, value: string) => useSettings.getState().set(key, value);

beforeEach(() => {
  useSettings.setState({ values: {} });
});

describe("settings accessors (parsing lives in the store)", () => {
  it("osNotifications defaults on and reads 'false' as off", () => {
    expect(getOsNotificationsEnabled()).toBe(true); // unset → default on
    set("osNotifications", "false");
    expect(getOsNotificationsEnabled()).toBe(false);
    set("osNotifications", "true");
    expect(getOsNotificationsEnabled()).toBe(true);
  });

  it("update.lastChecked parses to a number, 0 when absent or garbage", () => {
    expect(getUpdateLastChecked()).toBe(0);
    set("update.lastChecked", "1700000000000");
    expect(getUpdateLastChecked()).toBe(1700000000000);
    set("update.lastChecked", "nonsense");
    expect(getUpdateLastChecked()).toBe(0);
  });

  it("terminal font size rounds valid input and falls back on junk", () => {
    expect(getTerminalFontSize()).toBe(TERMINAL_FONT_SIZE_DEFAULT);
    set("terminalFontSize", "14");
    expect(getTerminalFontSize()).toBe(14);
    set("terminalFontSize", "13.6");
    expect(getTerminalFontSize()).toBe(14); // rounded
    set("terminalFontSize", "-3");
    expect(getTerminalFontSize()).toBe(TERMINAL_FONT_SIZE_DEFAULT); // not positive
  });
});
