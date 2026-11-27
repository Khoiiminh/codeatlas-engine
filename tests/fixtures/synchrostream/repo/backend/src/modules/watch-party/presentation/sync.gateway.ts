import type { PartyRoomRepository } from "../application/ports";
import { recordDrift } from "./missing-sync-metrics";

export type SyncEvent = { type: "play" | "pause" | "seek"; roomCode: string; positionMs: number };

export class SyncGateway {
    constructor(private readonly rooms: PartyRoomRepository) {}

    async handle(event: SyncEvent): Promise<void> {
        const room = await this.rooms.findByCode(event.roomCode);
        if (room) {
            recordDrift(event.roomCode, event.positionMs);
        }
    }

    async loadHandler(type: SyncEvent["type"]): Promise<unknown> {
        return import(`./handlers/${type}`);
    }
}
