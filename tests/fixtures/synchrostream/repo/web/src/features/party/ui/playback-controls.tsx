import { formatDuration, formatViewerCount } from "@/shared/lib";

export interface PlaybackControlsProps {
    readonly positionMs: number;
    readonly viewerCount: number;
    readonly onToggle: () => void;
}

export function PlaybackControls({ positionMs, viewerCount, onToggle }: PlaybackControlsProps) {
    return (
        <div className="playback-controls">
            <button onClick={onToggle}>Play / pause</button>
            <span>{formatDuration(positionMs)}</span>
            <span>{formatViewerCount(viewerCount)}</span>
        </div>
    );
}
