import { cleanup, render, screen, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import type { ComponentProps } from "react";
import { afterEach, describe, expect, it, vi } from "vitest";

import type { DesktopSnapshot } from "./commandAdapter";
import { DesktopCalendarContext } from "./desktopCalendarContext";
import { formatCalendarEventMetadata } from "./desktopWorkspaceState";

type CalendarContext = DesktopSnapshot["calendarContext"];
type CalendarEvent = CalendarContext["upcomingEvents"][number];

function calendarEvent(overrides: Partial<CalendarEvent> = {}): CalendarEvent {
  return {
    id: "event-1",
    title: "Design Review",
    calendarTitle: "Work",
    startsAtMs: Date.UTC(2026, 6, 8, 9, 0),
    endsAtMs: Date.UTC(2026, 6, 8, 10, 0),
    isAllDay: false,
    isRecurring: false,
    privacy: "Unknown",
    overlapState: "None",
    attachable: true,
    safetyNote:
      "Privacy classification is unavailable from EventKit; confirm this event title is safe before attaching.",
    ...overrides,
  };
}

function calendarContext(overrides: Partial<CalendarContext> = {}): CalendarContext {
  return {
    source: "AppleCalendar",
    permissionState: "Granted",
    availabilityState: "Ready",
    message: "Apple Calendar access is granted; no upcoming events found in the next 24 hours.",
    setupGuidance:
      "Upcoming local events are read-only until you explicitly attach one as meeting context.",
    upcomingEvents: [],
    autoStartEnabled: false,
    ...overrides,
  };
}

function renderCalendarContext(
  overrides: Partial<ComponentProps<typeof DesktopCalendarContext>> = {},
) {
  const props: ComponentProps<typeof DesktopCalendarContext> = {
    context: calendarContext(),
    label: "Calendar context ready",
    tone: "ready",
    pendingCommand: null,
    requestCalendarDisabled: false,
    requestCalendarTitle: "Request macOS Apple Calendar access for future manual event context.",
    canAttachEvents: true,
    hasSelectedMeeting: true,
    onRequestCalendarAccess: vi.fn(),
    onAttachCalendarEvent: vi.fn(),
    ...overrides,
  };

  return {
    ...render(<DesktopCalendarContext {...props} />),
    props,
  };
}

afterEach(() => {
  cleanup();
});

describe("DesktopCalendarContext", () => {
  it("renders the wrapper, App-supplied status, calendar guidance, no-events fallback, and auto-start note", () => {
    const { container } = renderCalendarContext({
      context: calendarContext({
        permissionState: "Denied",
        availabilityState: "Unavailable",
        message: "Apple Calendar access is denied.",
        setupGuidance: "Enable Calendar permission in System Settings before using event context.",
      }),
      label: "Calendar access denied",
      tone: "blocked",
    });

    const context = screen.getByLabelText("Calendar context");
    expect(container.firstElementChild).toBe(context);
    expect(context).toHaveClass("calendar-context");

    const readinessItem = context.querySelector(".readiness-item");
    expect(readinessItem).toHaveClass("blocked");
    expect(within(context).getByText("Calendar access denied")).toHaveClass("status-pill", "blocked");
    expect(within(context).getByText("Apple Calendar access is denied.")).toBeInTheDocument();
    expect(
      within(context).getByText("Enable Calendar permission in System Settings before using event context."),
    ).toBeInTheDocument();
    expect(within(context).getByText("No upcoming calendar events loaded.")).toBeInTheDocument();
    expect(within(context).getByText("Auto-start disabled.")).toBeInTheDocument();
  });

  it("renders upcoming events with formatter metadata and safety notes while omitting non-attachable buttons", () => {
    const unknownEvent = calendarEvent({
      id: "event-unknown",
      title: "Design Review",
      attachable: false,
      safetyNote: "Overlaps another event; attachment is disabled until ambiguity handling is implemented.",
      overlapState: "Overlapping",
    });
    const recurringPrivateEvent = calendarEvent({
      id: "event-private",
      title: "Private Planning",
      calendarTitle: "Leadership",
      startsAtMs: Date.UTC(2026, 6, 8, 9, 30),
      endsAtMs: Date.UTC(2026, 6, 8, 10, 30),
      isRecurring: true,
      privacy: "Private",
      overlapState: "Overlapping",
      attachable: false,
      safetyNote: "Recurring event; attachment is disabled until recurrence handling is implemented.",
    });

    renderCalendarContext({
      context: calendarContext({
        upcomingEvents: [unknownEvent, recurringPrivateEvent],
      }),
    });

    const context = screen.getByLabelText("Calendar context");
    const eventList = context.querySelector(".calendar-event-list");
    expect(eventList).toBeInTheDocument();
    expect(within(context).getByText("Design Review")).toBeInTheDocument();
    expect(within(context).getByText(formatCalendarEventMetadata(unknownEvent))).toBeInTheDocument();
    expect(
      within(context).getByText(
        "Overlaps another event; attachment is disabled until ambiguity handling is implemented.",
      ),
    ).toBeInTheDocument();
    expect(within(context).getByText("Private Planning")).toBeInTheDocument();
    expect(within(context).getByText(formatCalendarEventMetadata(recurringPrivateEvent))).toBeInTheDocument();
    expect(
      within(context).getByText("Recurring event; attachment is disabled until recurrence handling is implemented."),
    ).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: /attach/i })).not.toBeInTheDocument();
  });

  it("confirms and attaches an Unknown privacy event with the exact event payload", async () => {
    const user = userEvent.setup();
    const onAttachCalendarEvent = vi.fn();
    const event = calendarEvent({ privacy: "Unknown", attachable: true });
    renderCalendarContext({
      context: calendarContext({ upcomingEvents: [event] }),
      onAttachCalendarEvent,
    });

    const button = screen.getByRole("button", { name: "Confirm privacy and attach" });
    expect(button).toHaveAttribute(
      "title",
      "Confirm this unknown-privacy event is safe to store as meeting context.",
    );

    await user.click(button);

    expect(onAttachCalendarEvent).toHaveBeenCalledTimes(1);
    expect(onAttachCalendarEvent).toHaveBeenCalledWith(event);
  });

  it("attaches a known privacy event with the exact event payload", async () => {
    const user = userEvent.setup();
    const onAttachCalendarEvent = vi.fn();
    const event = calendarEvent({ privacy: "Private", attachable: true });
    renderCalendarContext({
      context: calendarContext({ upcomingEvents: [event] }),
      onAttachCalendarEvent,
    });

    const button = screen.getByRole("button", { name: "Attach to meeting" });
    expect(button).toHaveAttribute("title", "Attach this event as meeting context.");

    await user.click(button);

    expect(onAttachCalendarEvent).toHaveBeenCalledTimes(1);
    expect(onAttachCalendarEvent).toHaveBeenCalledWith(event);
  });

  it("disables attach with the no-selected-meeting title when no meeting can receive context", () => {
    const event = calendarEvent({ attachable: true });
    renderCalendarContext({
      context: calendarContext({ upcomingEvents: [event] }),
      canAttachEvents: false,
      hasSelectedMeeting: false,
    });

    const button = screen.getByRole("button", { name: "Confirm privacy and attach" });
    expect(button).toBeDisabled();
    expect(button).toHaveAttribute("title", "Select a meeting before attaching calendar context.");
  });

  it("renders the request-calendar button only for NotRequested and delegates disabled state, title, and clicks", async () => {
    const user = userEvent.setup();
    const onRequestCalendarAccess = vi.fn();
    const { rerender, props } = renderCalendarContext({
      context: calendarContext({ permissionState: "NotRequested", availabilityState: "PermissionRequired" }),
      requestCalendarDisabled: true,
      requestCalendarTitle: "Desktop commands are unavailable.",
      onRequestCalendarAccess,
    });

    const disabledButton = screen.getByRole("button", { name: "Request calendar access" });
    expect(disabledButton).toBeDisabled();
    expect(disabledButton).toHaveAttribute("title", "Desktop commands are unavailable.");

    rerender(
      <DesktopCalendarContext
        {...props}
        context={calendarContext({ permissionState: "NotRequested", availabilityState: "PermissionRequired" })}
        requestCalendarDisabled={false}
        requestCalendarTitle="Request macOS Apple Calendar access for future manual event context."
      />,
    );

    await user.click(screen.getByRole("button", { name: "Request calendar access" }));
    expect(onRequestCalendarAccess).toHaveBeenCalledTimes(1);

    rerender(
      <DesktopCalendarContext
        {...props}
        context={calendarContext({ permissionState: "Granted", availabilityState: "Ready" })}
      />,
    );
    expect(screen.queryByRole("button", { name: "Request calendar access" })).not.toBeInTheDocument();
  });

  it("swaps request and attach labels while the matching calendar command is pending", () => {
    const event = calendarEvent({ privacy: "Public", attachable: true });
    const { rerender, props } = renderCalendarContext({
      context: calendarContext({
        permissionState: "NotRequested",
        availabilityState: "PermissionRequired",
        upcomingEvents: [event],
      }),
      pendingCommand: "request-calendar",
    });

    expect(screen.getByRole("button", { name: "Requesting calendar" })).toBeInTheDocument();

    rerender(
      <DesktopCalendarContext
        {...props}
        context={calendarContext({ upcomingEvents: [event] })}
        pendingCommand="attach-calendar"
      />,
    );

    expect(screen.getByRole("button", { name: "Attaching" })).toBeInTheDocument();
  });
});
