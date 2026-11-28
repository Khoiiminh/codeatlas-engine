import React from "react";

export class Panel extends React.Component {
    static defaultProps = { open: false };
    #renders = 0;
    state = { open: false };

    get isOpen() {
        return this.state.open;
    }

    #count() {
        this.#renders += 1;
    }

    render() {
        this.#count();
        return <div>{this.props.children}</div>;
    }
}

export function* ids() {
    yield 1;
}

export async function load() {
    return 1;
}

export const Small = () => <span />;
var legacy = 1;
var legacy = 2;
let { a: [b] } = { a: [1] };
