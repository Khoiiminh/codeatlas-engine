const response = await fetch("https://example.invalid/data.json");

class Counter {
    #count = 0;
    static #instances = 0;
    static {
        Counter.#instances += 1;
    }
    increment() {
        return ++this.#count;
    }
}

const pattern = /^[a-z]+\/(\d{2,})$/giu;
const label = `count: ${new Counter().increment()}`;
export { response, pattern, label };
