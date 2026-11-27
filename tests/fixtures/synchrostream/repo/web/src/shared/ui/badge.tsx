export interface BadgeProps {
    readonly label: string;
    readonly tone?: "live" | "muted";
}

export function Badge({ label, tone = "muted" }: BadgeProps) {
    return <span className={`badge badge-${tone}`}>{label}</span>;
}
