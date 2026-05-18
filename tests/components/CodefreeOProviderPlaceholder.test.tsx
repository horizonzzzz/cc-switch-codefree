import { render, screen } from "@testing-library/react";
import { CodefreeOProviderPlaceholder } from "@/components/providers/CodefreeOProviderPlaceholder";

vi.mock("react-i18next", () => ({
  useTranslation: () => ({
    t: (key: string, options?: { defaultValue?: string }) =>
      options?.defaultValue ?? key,
  }),
}));

describe("CodefreeOProviderPlaceholder", () => {
  it("renders the placeholder with correct text", () => {
    render(<CodefreeOProviderPlaceholder />);

    expect(
      screen.getByText("Provider Management Not Supported"),
    ).toBeInTheDocument();
    expect(
      screen.getByText(
        "This agent does not support provider management. The provider page is displayed for visual consistency with other agents only.",
      ),
    ).toBeInTheDocument();
  });

  it("displays the shield icon", () => {
    const { container } = render(<CodefreeOProviderPlaceholder />);
    const svg = container.querySelector("svg");
    expect(svg).toBeInTheDocument();
  });
});
