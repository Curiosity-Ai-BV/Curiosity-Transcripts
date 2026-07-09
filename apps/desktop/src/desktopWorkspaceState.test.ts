import { describe, expect, it } from "vitest";

import { getMockDesktopSnapshot } from "./commandAdapter";
import type { CommandJobView, DesktopSnapshot } from "./commandAdapter";
import {
  calendarContextLabel,
  calendarContextTone,
  commandAllowedDuringBusy,
  formatCalendarEventMetadata,
  ollamaSetupLabel,
  ollamaSetupTone,
  preserveCommandJobProgress,
  resolveSelectedMeetingId,
  whisperSetupLabel,
  whisperSetupTone,
} from "./desktopWorkspaceState";

describe("desktop workspace state helpers", () => {
  it("resolves selected meetings from backend selection, current selection, first meeting, then null", () => {
    const snapshot = getMockDesktopSnapshot();

    expect(resolveSelectedMeetingId(snapshot, "design-standup")).toBe("circuit-review");
    expect(resolveSelectedMeetingId({ ...snapshot, selectedMeetingId: "missing" }, "design-standup")).toBe(
      "design-standup",
    );
    expect(resolveSelectedMeetingId({ ...snapshot, selectedMeetingId: null }, "missing")).toBe(
      "circuit-review",
    );
    expect(resolveSelectedMeetingId({ ...snapshot, meetings: [], selectedMeetingId: null }, "missing")).toBeNull();
  });

  it("keeps terminal command job progress from regressing to a stale running snapshot", () => {
    const completeJob: CommandJobView = {
      id: "transcription-circuit-review-1",
      kind: "Transcription",
      meetingId: "circuit-review",
      state: "Complete",
      cancelRequested: false,
      startedAtMs: 1_700_000_000_000,
    };
    const staleRunningJob: CommandJobView = {
      ...completeJob,
      state: "Running",
    };
    const terminalUpdateJob: CommandJobView = {
      ...completeJob,
      state: "Failed",
      lastError: "Whisper worker exited.",
    };
    const newRunningJob: CommandJobView = {
      ...staleRunningJob,
      id: "transcription-circuit-review-2",
    };
    const current: DesktopSnapshot = {
      ...getMockDesktopSnapshot(),
      transcriptionJob: completeJob,
    };
    const staleRunning: DesktopSnapshot = {
      ...current,
      transcriptionJob: staleRunningJob,
    };
    const terminalUpdate: DesktopSnapshot = {
      ...current,
      transcriptionJob: terminalUpdateJob,
    };
    const newJobUpdate: DesktopSnapshot = {
      ...staleRunning,
      transcriptionJob: newRunningJob,
    };

    expect(preserveCommandJobProgress(current, staleRunning).transcriptionJob).toBe(current.transcriptionJob);
    expect(preserveCommandJobProgress(current, terminalUpdate).transcriptionJob).toBe(
      terminalUpdate.transcriptionJob,
    );
    expect(preserveCommandJobProgress(current, newJobUpdate).transcriptionJob).toBe(newJobUpdate.transcriptionJob);
  });

  it("only allows matching cancellation commands while busy", () => {
    expect(commandAllowedDuringBusy("cancel-transcription", "transcribe")).toBe(true);
    expect(commandAllowedDuringBusy("cancel-summary", "summary")).toBe(true);
    expect(commandAllowedDuringBusy("cancel-summary", "transcribe")).toBe(false);
    expect(commandAllowedDuringBusy("cancel-transcription", "summary")).toBe(false);
    expect(commandAllowedDuringBusy("delete", "summary")).toBe(false);
    expect(commandAllowedDuringBusy("cancel-summary", null)).toBe(false);
  });

  it("formats model and calendar readiness labels without hiding blocked setup state", () => {
    const snapshot = getMockDesktopSnapshot();
    const blockedOllama = {
      ...snapshot.setupGuidance.ollama,
      availability: "MissingModelAtLastTest" as const,
    };
    const readyCalendar = {
      ...snapshot.calendarContext,
      permissionState: "Granted" as const,
      availabilityState: "Ready" as const,
    };

    expect(whisperSetupLabel("ReadablePath")).toBe("Whisper path readable");
    expect(whisperSetupTone("ReadablePath")).toBe("warn");
    expect(ollamaSetupLabel(blockedOllama)).toBe("Ollama model missing");
    expect(ollamaSetupTone(blockedOllama)).toBe("blocked");
    expect(calendarContextLabel(readyCalendar)).toBe("Calendar context ready");
    expect(calendarContextTone(readyCalendar)).toBe("ready");
  });

  it("formats calendar event metadata with privacy and overlap context", () => {
    expect(
      formatCalendarEventMetadata({
        id: "event-1",
        title: "Design Review",
        calendarTitle: "Work",
        startsAtMs: Date.UTC(2026, 0, 2, 9, 30),
        endsAtMs: Date.UTC(2026, 0, 2, 10, 0),
        isAllDay: false,
        isRecurring: true,
        privacy: "Private",
        overlapState: "Overlapping",
        attachable: true,
        safetyNote: "",
      }),
    ).toContain("Work / Private privacy / Overlapping / Recurring");
  });
});
