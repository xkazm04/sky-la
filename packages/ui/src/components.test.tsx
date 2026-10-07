import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { resolveAppearance } from "./appearance";
import { Badge } from "./components/Badge";
import { Select, TextField } from "./components/Field";
import { Kbd } from "./components/Kbd";
import { SegmentedControl } from "./components/SegmentedControl";
import { formatMinor } from "./format";

describe("Badge", () => {
  it("always pairs a label with an icon", () => {
    const { container } = render(<Badge tone="negative">Overdue</Badge>);
    expect(screen.getByText("Overdue")).toBeTruthy();
    const icon = container.querySelector("svg");
    expect(icon?.getAttribute("aria-hidden")).toBe("true");
  });
});

describe("Kbd", () => {
  it("uses symbols on macOS and words elsewhere", () => {
    render(<Kbd keys="mod+k" mac />);
    expect(screen.getByLabelText("Command K").textContent).toBe("⌘K");
    render(<Kbd keys={["mod", "shift", "p"]} mac={false} />);
    expect(screen.getByLabelText("Ctrl+Shift+P").textContent).toBe("CtrlShiftP");
  });
});

describe("SegmentedControl", () => {
  it("reports the chosen segment", () => {
    const onChange = vi.fn();
    render(
      <SegmentedControl
        label="Basis"
        segments={[
          { id: "accrual", label: "Accrual" },
          { id: "cash", label: "Cash" },
        ]}
        value="accrual"
        onChange={onChange}
      />,
    );
    expect(screen.getByRole("radio", { name: "Accrual" }).getAttribute("aria-checked")).toBe(
      "true",
    );
    fireEvent.click(screen.getByRole("radio", { name: "Cash" }));
    expect(onChange).toHaveBeenCalledWith("cash");
  });
});

describe("formatMinor", () => {
  it("lays out the core's integer minor units in Czech style", () => {
    expect(formatMinor(8_470_000)).toBe("84 700,00 Kč");
    expect(formatMinor(5, "EUR")).toBe("0,05 €");
    expect(formatMinor(-123_456_789, "CZK", { symbol: false })).toBe("−1 234 567,89");
    expect(() => formatMinor(1.5)).toThrow(RangeError);
  });
});

describe("appearance", () => {
  it("follows the system only when asked to", () => {
    expect(resolveAppearance("system", true)).toBe("dark");
    expect(resolveAppearance("system", false)).toBe("light");
    expect(resolveAppearance("light", true)).toBe("light");
  });
});

describe("TextField", () => {
  it("labels the input and marks it invalid with its message", () => {
    const onChange = vi.fn();
    render(
      <TextField
        label="Unit price"
        value="1450.00"
        onChange={onChange}
        errorMessage="Use 1 450,00"
      />,
    );
    const input = screen.getByLabelText("Unit price") as HTMLInputElement;
    expect(input.getAttribute("aria-invalid")).toBe("true");
    fireEvent.change(input, { target: { value: "1 450,00" } });
    expect(onChange).toHaveBeenCalledWith("1 450,00");
  });

  it("keeps a hidden label for screen readers", () => {
    render(<TextField label="Line 2 unit" labelHidden defaultValue="ks" />);
    expect((screen.getByLabelText("Line 2 unit") as HTMLInputElement).value).toBe("ks");
  });
});

describe("Select", () => {
  it("shows the chosen option's label", () => {
    render(
      <Select
        label="VAT"
        value="OUT12"
        onChange={() => {}}
        options={[
          { id: "OUT21", label: "21 %", detail: "standard" },
          { id: "OUT12", label: "12 %", detail: "reduced" },
        ]}
      />,
    );
    const button = screen.getByRole("button");
    expect(button.textContent).toContain("12 %");
    expect(button.textContent).not.toContain("reduced");
  });
});
