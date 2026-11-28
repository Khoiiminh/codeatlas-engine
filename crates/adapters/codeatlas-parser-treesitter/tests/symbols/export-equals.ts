class Client {
    connect(): void {}
}

namespace Client {
    export const defaultPort = 80;
}

function notExported(): void {}

export = Client;
