import { render, screen, fireEvent, act } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { t } from "@/i18n";
import { Onboarding } from "./Onboarding";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("Onboarding", () => {
  it("renders first step", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    expect(screen.getByText(t("onboarding.step1Title"))).toBeInTheDocument();
  });

  it("renders skip button", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    expect(screen.getByText(t("onboarding.skip"))).toBeInTheDocument();
  });

  it("renders next button", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    expect(screen.getByText(t("onboarding.next"))).toBeInTheDocument();
  });

  it("calls onComplete when skip is clicked", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    fireEvent.click(screen.getByText(t("onboarding.skip")));
    expect(onComplete).toHaveBeenCalledTimes(1);
  });

  it("navigates to next step when next is clicked", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.next")));
    });

    act(() => {
      vi.advanceTimersByTime(350);
    });

    expect(screen.getByText(t("onboarding.step2Title"))).toBeInTheDocument();
    expect(onComplete).not.toHaveBeenCalled();
  });

  it("navigates through all steps", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.next")));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    expect(screen.getByText(t("onboarding.step2Title"))).toBeInTheDocument();

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.next")));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    expect(screen.getByText(t("onboarding.step3Title"))).toBeInTheDocument();

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.next")));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });
    expect(screen.getByText(t("onboarding.step4Title"))).toBeInTheDocument();
  });

  it("shows finish button on last step", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.next")));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.next")));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.next")));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });

    expect(screen.getByText(t("onboarding.start"))).toBeInTheDocument();
  });

  it("calls onComplete on last step click", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.next")));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.next")));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.next")));
    });
    act(() => {
      vi.advanceTimersByTime(350);
    });

    act(() => {
      fireEvent.click(screen.getByText(t("onboarding.start")));
    });

    expect(onComplete).toHaveBeenCalledTimes(1);
  });

  it("renders keyboard hint", () => {
    const onComplete = vi.fn();
    render(<Onboarding onComplete={onComplete} />);
    expect(screen.getByText(t("onboarding.escHint", { key: "Esc" }))).toBeInTheDocument();
  });
});
