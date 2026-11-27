export type MemberRole = "host" | "guest";

export interface PartyMember {
    readonly userId: string;
    readonly role: MemberRole;
    readonly joinedAt: Date;
}

export interface HostMember extends PartyMember {
    readonly role: "host";
}
