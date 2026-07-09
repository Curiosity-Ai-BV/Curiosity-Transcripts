import { CalendarPlus } from "@phosphor-icons/react";

import type { DesktopSnapshot, Tone } from "./commandAdapter";
import { StatusPill } from "./desktopWorkspaceComponents";
import { formatCalendarEventMetadata } from "./desktopWorkspaceState";
import type { PendingCommand } from "./desktopWorkspaceState";

type CalendarContext = DesktopSnapshot["calendarContext"];
type CalendarEvent = CalendarContext["upcomingEvents"][number];

export interface DesktopCalendarContextProps {
  context: CalendarContext;
  label: string;
  tone: Tone;
  pendingCommand: PendingCommand;
  requestCalendarDisabled: boolean;
  requestCalendarTitle: string;
  canAttachEvents: boolean;
  hasSelectedMeeting: boolean;
  onRequestCalendarAccess(): void;
  onAttachCalendarEvent(event: CalendarEvent): void;
}

export function DesktopCalendarContext({
  context,
  label,
  tone,
  pendingCommand,
  requestCalendarDisabled,
  requestCalendarTitle,
  canAttachEvents,
  hasSelectedMeeting,
  onRequestCalendarAccess,
  onAttachCalendarEvent,
}: DesktopCalendarContextProps) {
  return (
    <div className="calendar-context" aria-label="Calendar context">
      <div className={`readiness-item ${tone}`}>
        <div className="readiness-heading">
          <StatusPill tone={tone} label={label} />
        </div>
        <p>{context.message}</p>
        <p>{context.setupGuidance}</p>
        {context.upcomingEvents.length > 0 ? (
          <div className="calendar-event-list">
            {context.upcomingEvents.map((event) => (
              <div key={event.id} className="calendar-event-row">
                <strong>{event.title}</strong>
                <span>{formatCalendarEventMetadata(event)}</span>
                <small>{event.safetyNote}</small>
                {event.attachable ? (
                  <button
                    type="button"
                    className="button"
                    disabled={!canAttachEvents}
                    title={calendarAttachTitle(event, hasSelectedMeeting)}
                    onClick={() => onAttachCalendarEvent(event)}
                  >
                    <CalendarPlus size={16} weight="regular" />
                    {calendarAttachLabel(event, pendingCommand)}
                  </button>
                ) : null}
              </div>
            ))}
          </div>
        ) : (
          <small>No upcoming calendar events loaded.</small>
        )}
        <small>Auto-start disabled.</small>
        {context.permissionState === "NotRequested" ? (
          <button
            type="button"
            className="button"
            disabled={requestCalendarDisabled}
            title={requestCalendarTitle}
            onClick={onRequestCalendarAccess}
          >
            {pendingCommand === "request-calendar" ? "Requesting calendar" : "Request calendar access"}
          </button>
        ) : null}
      </div>
    </div>
  );
}

function calendarAttachLabel(event: CalendarEvent, pendingCommand: PendingCommand) {
  if (pendingCommand === "attach-calendar") {
    return "Attaching";
  }
  if (event.privacy === "Unknown") {
    return "Confirm privacy and attach";
  }
  return "Attach to meeting";
}

function calendarAttachTitle(event: CalendarEvent, hasSelectedMeeting: boolean) {
  if (!hasSelectedMeeting) {
    return "Select a meeting before attaching calendar context.";
  }
  if (event.privacy === "Unknown") {
    return "Confirm this unknown-privacy event is safe to store as meeting context.";
  }
  return "Attach this event as meeting context.";
}
