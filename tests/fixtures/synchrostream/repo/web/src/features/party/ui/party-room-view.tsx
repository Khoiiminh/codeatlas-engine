import { useState } from "react";
import { Ui } from "@/shared/ui";
import { usePartySync } from "../model/use-party-sync";
import { PlaybackControls } from "./playback-controls";

export interface PartyRoomViewProps {
    readonly roomCode: string;
}

export function PartyRoomView({ roomCode }: PartyRoomViewProps) {
    const { summary, status } = usePartySync(roomCode);
    const [positionMs, setPositionMs] = useState(0);

    return (
        <section>
            <h1>Room {roomCode}</h1>
            <Ui.Badge label={status} tone={status === "live" ? "live" : "muted"} />
            <PlaybackControls
                positionMs={positionMs}
                viewerCount={summary?.viewerCount ?? 0}
                onToggle={() => setPositionMs((value) => value + 1000)}
            />
        </section>
    );
}
