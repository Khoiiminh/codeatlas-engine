import { httpGet } from "@/shared/lib/http";

export interface PartySummary {
    readonly roomCode: string;
    readonly hostName: string;
    readonly viewerCount: number;
}

export function fetchPartySummary(roomCode: string): Promise<PartySummary> {
    return httpGet<PartySummary>(`/api/parties/${roomCode}`);
}
