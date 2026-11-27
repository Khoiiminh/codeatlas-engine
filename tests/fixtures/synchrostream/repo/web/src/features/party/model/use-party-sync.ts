import { useEffect, useState } from "react";
import { fetchPartySummary, type PartySummary } from "../api/party-api";

export type SyncStatus = "connecting" | "live" | "offline";

export function usePartySync(roomCode: string): { summary: PartySummary | undefined; status: SyncStatus } {
    const [summary, setSummary] = useState<PartySummary>();
    const [status, setStatus] = useState<SyncStatus>("connecting");

    useEffect(() => {
        fetchPartySummary(roomCode)
            .then((next) => {
                setSummary(next);
                setStatus("live");
            })
            .catch(() => setStatus("offline"));
    }, [roomCode]);

    return { summary, status };
}
