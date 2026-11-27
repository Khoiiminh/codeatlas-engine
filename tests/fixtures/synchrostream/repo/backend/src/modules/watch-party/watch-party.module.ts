import { Module } from "@nestjs/common";
import { JoinPartyUseCase } from "./application/join-party.use-case";
import { PARTY_ROOM_REPOSITORY } from "./application/ports";
import { InMemoryPartyRoomRepository } from "./infrastructure/in-memory-party-room.repository";
import { WatchPartyController } from "./presentation/watch-party.controller";

@Module({
    controllers: [WatchPartyController],
    providers: [JoinPartyUseCase, { provide: PARTY_ROOM_REPOSITORY, useClass: InMemoryPartyRoomRepository }],
})
export class WatchPartyModule {}
