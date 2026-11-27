import { Injectable } from "@nestjs/common";
import type { PartyRoomRepository } from "../application/ports";
import type { PartyRoom, RoomCode } from "../domain/party-room";

@Injectable()
export class InMemoryPartyRoomRepository implements PartyRoomRepository {
    private readonly rooms = new Map<RoomCode, PartyRoom>();

    async findByCode(code: RoomCode): Promise<PartyRoom | undefined> {
        return this.rooms.get(code);
    }

    async save(room: PartyRoom): Promise<void> {
        this.rooms.set(room.code, room);
    }
}
