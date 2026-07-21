import { beforeEach, describe, expect, it } from "vitest";
import { errMsg, markEventError, toastError } from "./errors";
import { useToast } from "../stores/toast";

beforeEach(() => {
  useToast.setState({ toasts: [] });
});

describe("errMsg", () => {
  it("unwraps a string rejection, an Error, and anything else", () => {
    expect(errMsg("boom")).toBe("boom");
    expect(errMsg(new Error("bad"))).toBe("bad");
    expect(errMsg(42)).toBe("42");
  });
});

describe("toastError dedupe against the site-event stream", () => {
  it("shows an error toast for a fresh failure", () => {
    toastError("disk full", "Create site");
    const toasts = useToast.getState().toasts;
    expect(toasts).toHaveLength(1);
    expect(toasts[0].kind).toBe("error");
    expect(toasts[0].title).toBe("Create site");
    expect(toasts[0].message).toBe("disk full");
  });

  it("suppresses a toast the event stream just showed for the same message", () => {
    markEventError("disk full");
    toastError("disk full", "Create site");
    expect(useToast.getState().toasts).toHaveLength(0);
  });

  it("still toasts an unrelated failure after an event error", () => {
    markEventError("disk full");
    toastError("network down", "Push DB");
    expect(useToast.getState().toasts).toHaveLength(1);
  });
});
