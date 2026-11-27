import { Module } from "@nestjs/common";
import { WatchPartyModule } from "./modules/watch-party/watch-party.module";

@Module({ imports: [WatchPartyModule] })
export class AppModule {}
