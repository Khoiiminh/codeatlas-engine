import { DomainError, type Clock } from "../../../shared/kernel";
import type { MemberRole, PartyMember } from "./party-member";

export type RoomCode = string;

export enum PlaybackState {
    Paused = "paused",
    Playing = "playing",
}

export class PlaybackPermissionError extends DomainError {
    constructor(roomCode: RoomCode) {
        super("playback_permission", `only the host can control playback in ${roomCode}`);
    }
}

export class PartyRoom {
    private readonly members: PartyMember[] = [];
    private state: PlaybackState = PlaybackState.Paused;

    private constructor(
        readonly code: RoomCode,
        private readonly hostId: string,
        private readonly clock: Clock,
    ) {}

    static open(code: RoomCode, hostId: string, clock: Clock): PartyRoom {
        const room = new PartyRoom(code, hostId, clock);
        room.join(hostId);
        return room;
    }

    join(userId: string): PartyMember {
        const role: MemberRole = userId === this.hostId ? "host" : "guest";
        const member: PartyMember = { userId, role, joinedAt: this.clock.now() };
        this.members.push(member);
        return member;
    }

    play(userId: string): void {
        this.assertHost(userId);
        this.state = PlaybackState.Playing;
    }

    pause(userId: string): void {
        this.assertHost(userId);
        this.state = PlaybackState.Paused;
    }

    get memberCount(): number {
        return this.members.length;
    }

    private assertHost(userId: string): void {
        if (userId !== this.hostId) {
            throw new PlaybackPermissionError(this.code);
        }
    }
}
