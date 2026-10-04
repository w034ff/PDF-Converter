import { render, screen } from "@testing-library/react";
import { App } from "./App";

describe("App", () => {
  it("shows the app name", () => {
    render(<App />);
    expect(
      screen.getByRole("heading", { name: "PDF Converter" }),
    ).toBeInTheDocument();
  });
});
