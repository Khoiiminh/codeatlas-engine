import { Body, Controller, Param, Post } from "@nestjs/common";
import { JoinPartyUseCase } from "../application/join-party.use-case";

class JoinPartyBody {
    userId!: string;
}

@Controller("parties")
export class WatchPartyController {
    constructor(private readonly joinParty: JoinPartyUseCase) {}

    @Post(":roomCode/members")
    async join(@Param("roomCode") roomCode: string, @Body() body: JoinPartyBody) {
        const result = await this.joinParty.execute({ roomCode, userId: body.userId });
        if (!result.ok) {
            throw new Error(result.error);
        }
        return result.value;
    }
}
