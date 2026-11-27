import { PartyRoomView } from "@/features/party/ui/party-room-view";
import { LegacyBanner } from "@/shared/ui/legacy-banner";

interface PageProps {
    readonly params: { readonly roomCode: string };
}

export default function PartyPage({ params }: PageProps) {
    return (
        <main>
            <LegacyBanner message="Watch parties are in beta" />
            <PartyRoomView roomCode={params.roomCode} />
        </main>
    );
}
