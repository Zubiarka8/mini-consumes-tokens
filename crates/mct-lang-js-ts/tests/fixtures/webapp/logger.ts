export function log(message: string): void {
    recordLog(message);
}

function recordLog(message: string): void {
    void message;
}

export const PREFIX = "[LOG]";
