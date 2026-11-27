import { Inject, Injectable } from "@nestjs/common";
import { err, ok, type Result } from "../../../shared/kernel";
import type { PartyMember } from "../domain/party-member";
import { PARTY_ROOM_REPOSITORY, type PartyRoomRepository } from "./ports";

export interface JoinPartyCommand {
    readonly roomCode: string;
    readonly userId: string;
}

@Injectable()
export class JoinPartyUseCase {
    constructor(@Inject(PARTY_ROOM_REPOSITORY) private readonly rooms: PartyRoomRepository) {}

    async execute(command: JoinPartyCommand): Promise<Result<PartyMember>> {
        const room = await this.rooms.findByCode(command.roomCode);
        if (!room) {
            return err(`room ${command.roomCode} not found`);
        }
        const member = room.join(command.userId);
        await this.rooms.save(room);
        return ok(member);
    }
}
