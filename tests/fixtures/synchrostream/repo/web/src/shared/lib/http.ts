export class HttpError extends Error {
    constructor(
        readonly status: number,
        message: string,
    ) {
        super(message);
    }
}

export async function httpGet<T>(url: string): Promise<T> {
    const response = await fetch(url);
    if (!response.ok) {
        const error = new HttpError(response.status, `GET ${url} failed`);
        reportToMonitoring(error);
        throw error;
    }
    return (await response.json()) as T;
}

export function formatDuration(ms: number): string {
    return `${Math.round(ms / 1000)}s`;
}
