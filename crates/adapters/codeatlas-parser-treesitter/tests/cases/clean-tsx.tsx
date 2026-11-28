import { useState, type ReactNode } from "react";

type Props<T> = { items: T[]; render: (item: T) => ReactNode };

export const List = <T,>({ items, render }: Props<T>) => {
    const [open, setOpen] = useState(false);
    return (
        <>
            <button onClick={() => setOpen(!open)} aria-expanded={open}>
                Toggle
            </button>
            {open && <ul {...{ role: "list" }}>{items.map((item, i) => <li key={i}>{render(item)}</li>)}</ul>}
        </>
    );
};
