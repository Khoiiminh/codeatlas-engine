import type { PartyRoom, RoomCode } from "../domain/party-room";

export const PARTY_ROOM_REPOSITORY = "PartyRoomRepository";

export interface PartyRoomRepository {
    findByCode(code: RoomCode): Promise<PartyRoom | undefined>;
    save(room: PartyRoom): Promise<void>;
}
