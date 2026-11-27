// Older helpers kept in JavaScript; TypeScript reads them through allowJs.
export function formatDuration(ms) {
    const minutes = Math.floor(ms / 60000);
    const seconds = Math.floor((ms % 60000) / 1000);
    return `${minutes}:${String(seconds).padStart(2, "0")}`;
}

export function formatViewerCount(count) {
    return count === 1 ? "1 viewer" : `${count} viewers`;
}
