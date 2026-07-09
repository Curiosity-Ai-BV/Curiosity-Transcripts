import { MagnifyingGlass } from "@phosphor-icons/react";

import type { MeetingView } from "./commandAdapter";
import { SkeletonList } from "./desktopWorkspaceComponents";

type MeetingPaneMeeting = Pick<MeetingView, "id" | "title" | "startedAt" | "duration" | "transcriptState">;

interface MeetingPaneProps {
  query: string;
  meetings: MeetingPaneMeeting[];
  selectedMeetingId: string | null;
  loading: boolean;
  onQueryChange(query: string): void;
  onSelectMeeting(meetingId: string): void;
}

export function MeetingPane({
  query,
  meetings,
  selectedMeetingId,
  loading,
  onQueryChange,
  onSelectMeeting,
}: MeetingPaneProps) {
  return (
    <aside className="meeting-pane" aria-label="Meetings">
      <div className="pane-heading">
        <p className="eyebrow">History</p>
        <h2>Meetings</h2>
      </div>
      <div className="search-block">
        <label htmlFor="meeting-search">Search meetings</label>
        <div className="search-control">
          <MagnifyingGlass size={16} weight="regular" />
          <input
            id="meeting-search"
            value={query}
            onChange={(event) => onQueryChange(event.target.value)}
            placeholder="Title or transcript text"
          />
        </div>
      </div>

      {loading ? <SkeletonList /> : null}

      <div className="meeting-list">
        {meetings.map((meeting) => {
          const selected = meeting.id === selectedMeetingId;
          return (
            <button
              type="button"
              key={meeting.id}
              className={selected ? "meeting-row selected" : "meeting-row"}
              aria-pressed={selected}
              aria-current={selected ? "page" : undefined}
              onClick={() => onSelectMeeting(meeting.id)}
            >
              <span className="meeting-title">{meeting.title}</span>
              <span className="meeting-meta">
                {meeting.startedAt} / {meeting.duration}
              </span>
              <span className="meeting-state">{meeting.transcriptState}</span>
            </button>
          );
        })}
      </div>

      {!loading && query && meetings.length === 0 ? <p className="empty-state">No meetings match this search.</p> : null}
    </aside>
  );
}
