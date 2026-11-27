// Build output left in the tree on purpose: scanners must skip dist/ (ENG-01).
import { NestFactory } from "@nestjs/core";
export async function bootstrap() {
    const app = await NestFactory.create({});
    await app.listen(3000);
}
