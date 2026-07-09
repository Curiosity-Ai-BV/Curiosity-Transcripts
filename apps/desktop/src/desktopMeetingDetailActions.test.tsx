import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { ExportFormat } from "./commandAdapter";
import { MeetingDetailActions, type MeetingDetailActionsProps } from "./desktopMeetingDetailActions";

function meetingDetailActionsProps(
  overrides: Partial<MeetingDetailActionsProps> = {},
): MeetingDetailActionsProps {
  return {
    selectedExportFormat: "json",
    selectedExportFormatLabel: "JSON",
    exportDisabled: false,
    deleteDisabled: false,
    exportButtonTitle: "Export the selected meeting as JSON.",
    deleteButtonTitle: "Delete app-private data for the selected meeting.",
    pendingCommand: null,
    onExportFormatChange: vi.fn(),
    onExport: vi.fn(),
    onDelete: vi.fn(),
    ...overrides,
  };
}

afterEach(() => {
  cleanup();
});

describe("MeetingDetailActions", () => {
  it("renders export format label/select/options, export button, and delete button", () => {
    const { container } = render(<MeetingDetailActions {...meetingDetailActionsProps()} />);

    expect(container.firstElementChild).toHaveClass("detail-actions");
    expect(screen.getByText("Export format").closest("label")).toHaveClass("export-format-field");
    expect(screen.getByLabelText("Export format")).toHaveValue("json");
    expect(
      screen.getAllByRole("option").map((option) => ({
        value: option.getAttribute("value"),
        label: option.textContent,
      })),
    ).toEqual([
      { value: "json", label: "JSON" },
      { value: "markdown", label: "Markdown" },
      { value: "srt", label: "SRT" },
    ]);
    expect(screen.getByRole("button", { name: "Export JSON" })).toHaveClass("button");
    expect(screen.getByRole("button", { name: "Delete private data" })).toHaveClass("button", "danger");
  });

  it("propagates export format changes with the selected format value", () => {
    const onExportFormatChange = vi.fn();

    render(<MeetingDetailActions {...meetingDetailActionsProps({ onExportFormatChange })} />);

    fireEvent.change(screen.getByLabelText("Export format"), { target: { value: "markdown" } });

    expect(onExportFormatChange).toHaveBeenCalledWith("markdown" satisfies ExportFormat);
  });

  it("calls export and delete callbacks from the right buttons", async () => {
    const user = userEvent.setup();
    const onExport = vi.fn();
    const onDelete = vi.fn();

    render(<MeetingDetailActions {...meetingDetailActionsProps({ onExport, onDelete })} />);

    await user.click(screen.getByRole("button", { name: "Export JSON" }));
    await user.click(screen.getByRole("button", { name: "Delete private data" }));

    expect(onExport).toHaveBeenCalledTimes(1);
    expect(onDelete).toHaveBeenCalledTimes(1);
  });

  it("applies disabled and title props to the export format select and action buttons", () => {
    render(
      <MeetingDetailActions
        {...meetingDetailActionsProps({
          exportDisabled: true,
          deleteDisabled: true,
          exportButtonTitle: "Export unavailable.",
          deleteButtonTitle: "Delete unavailable.",
        })}
      />,
    );

    expect(screen.getByLabelText("Export format")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Export JSON" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Export JSON" })).toHaveAttribute(
      "title",
      "Export unavailable.",
    );
    expect(screen.getByRole("button", { name: "Delete private data" })).toBeDisabled();
    expect(screen.getByRole("button", { name: "Delete private data" })).toHaveAttribute(
      "title",
      "Delete unavailable.",
    );
  });

  it("renders pending labels for export and delete states using the supplied export label", () => {
    const { rerender } = render(
      <MeetingDetailActions
        {...meetingDetailActionsProps({
          selectedExportFormat: "srt",
          selectedExportFormatLabel: "SRT",
          pendingCommand: "export",
        })}
      />,
    );

    expect(screen.getByRole("button", { name: "Exporting SRT" })).toBeInTheDocument();

    rerender(
      <MeetingDetailActions
        {...meetingDetailActionsProps({
          selectedExportFormat: "markdown",
          selectedExportFormatLabel: "Markdown",
          pendingCommand: "delete",
        })}
      />,
    );
    expect(screen.getByRole("button", { name: "Export Markdown" })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "Deleting private data" })).toBeInTheDocument();
  });
});
