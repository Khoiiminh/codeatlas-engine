// Bundlers accept JSX in .js files; the javascript grammar does too.
export function Greeting({ name }) {
    return <p className="greeting">Hello, {name}!</p>;
}
